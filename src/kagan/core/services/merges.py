"""Merge service operations - decoupled from UI."""

from __future__ import annotations

import asyncio
import logging
from contextlib import suppress
from dataclasses import dataclass
from enum import StrEnum
from typing import TYPE_CHECKING, Protocol

from kagan.core.adapters.db.session import get_required_session
from kagan.core.adapters.process import ProcessExecutionError, ProcessRetryPolicy, run_exec_checked
from kagan.core.models.enums import MergeStatus, MergeType, RejectionAction, TaskStatus
from kagan.core.time import utc_now

if TYPE_CHECKING:
    from kagan.core.adapters.db.repositories import ClosingAwareSessionFactory
    from kagan.core.adapters.db.schema import Task
    from kagan.core.adapters.git.operations import GitOperationsProtocol
    from kagan.core.config import KaganConfig
    from kagan.core.events import EventBus
    from kagan.core.services.automation import AutomationService
    from kagan.core.services.sessions import SessionService
    from kagan.core.services.tasks import TaskService
    from kagan.core.services.types import TaskLike
    from kagan.core.services.workspaces import WorkspaceService

log = logging.getLogger(__name__)


class MergeStrategy(StrEnum):
    """How to merge changes."""

    DIRECT = "direct"
    PULL_REQUEST = "pr"


@dataclass
class MergeResult:
    """Result of a merge operation."""

    repo_id: str
    repo_name: str
    strategy: MergeStrategy
    success: bool
    message: str
    pr_url: str | None = None
    commit_sha: str | None = None
    conflict_op: str | None = None
    conflict_files: list[str] | None = None


@dataclass(frozen=True)
class MergeRisk:
    """Simple risk summary used for merge gating."""

    score: int
    overlap_files: tuple[str, ...]
    commit_count: int
    changed_repo_count: int
    changed_file_count: int

    @property
    def high(self) -> bool:
        return self.score >= 2 or bool(self.overlap_files)


class MergeService(Protocol):
    """Protocol boundary for merge and cleanup lifecycle operations."""

    async def delete_task(self, task: TaskLike) -> tuple[bool, str]: ...

    async def merge_task(self, task: TaskLike) -> tuple[bool, str]: ...

    async def close_exploratory(self, task: TaskLike) -> tuple[bool, str]: ...

    async def apply_rejection_feedback(
        self,
        task: TaskLike,
        feedback: str | None,
        action: str = "backlog",
    ) -> Task: ...

    async def has_no_changes(self, task: TaskLike) -> bool: ...

    async def merge_repo(
        self,
        workspace_id: str,
        repo_id: str,
        *,
        strategy: MergeStrategy = MergeStrategy.DIRECT,
        pr_title: str | None = None,
        pr_body: str | None = None,
        commit_message: str | None = None,
    ) -> MergeResult: ...

    async def merge_all(
        self,
        workspace_id: str,
        *,
        strategy: MergeStrategy = MergeStrategy.DIRECT,
        skip_unchanged: bool = True,
        commit_message: str | None = None,
    ) -> list[MergeResult]: ...

    async def create_pr(
        self,
        workspace_id: str,
        repo_id: str,
        *,
        title: str,
        body: str,
        draft: bool = False,
    ) -> str: ...


class MergeServiceImpl:
    """Manages merge lifecycle operations without UI coupling."""

    _MERGE_QUIESCE_TIMEOUT_SECONDS = 5.0
    _MERGE_QUIESCE_POLL_SECONDS = 0.1

    def __init__(
        self,
        task_service: TaskService,
        worktrees: WorkspaceService,
        sessions: SessionService,
        automation: AutomationService,
        config: KaganConfig,
        session_factory: ClosingAwareSessionFactory | None = None,
        event_bus: EventBus | None = None,
        git_adapter: GitOperationsProtocol | None = None,
    ) -> None:
        self.tasks = task_service
        self.worktrees = worktrees
        self.workspace_service: WorkspaceService = worktrees
        self.sessions = sessions
        self.automation = automation
        self.config = config
        self._session_factory = session_factory
        self._events = event_bus
        self._git = git_adapter
        self._rebase_first_hints: dict[str, int] = {}

    def _is_task_runtime_active(self, task_id: str) -> bool:
        return self.automation.is_running(task_id) or self.automation.is_reviewing(task_id)

    async def _ensure_task_idle_before_merge(self, task_id: str) -> tuple[bool, str | None]:
        """Stop runtime activity and wait until task runtime is fully idle."""
        if not self._is_task_runtime_active(task_id):
            return True, None

        with suppress(Exception):
            await self.automation.stop_task(task_id)

        loop = asyncio.get_running_loop()
        deadline = loop.time() + self._MERGE_QUIESCE_TIMEOUT_SECONDS
        while self._is_task_runtime_active(task_id):
            if loop.time() >= deadline:
                message = "Task runtime is still active; wait for agent shutdown and retry merge."
                return False, message
            await asyncio.sleep(self._MERGE_QUIESCE_POLL_SECONDS)
        return True, None

    async def _get_latest_workspace_id(self, task_id: str) -> str | None:
        workspaces = await self.workspace_service.list_workspaces(task_id=task_id)
        return workspaces[0].id if workspaces else None

    async def delete_task(self, task: TaskLike) -> tuple[bool, str]:
        """Delete task with rollback-aware error handling.

        Returns:
            Tuple of (success, message) indicating result and reason.
        """
        steps_completed: list[str] = []
        try:
            if self.automation.is_running(task.id):
                await self.automation.stop_task(task.id)
            steps_completed.append("agent_stopped")

            await self.sessions.kill_session(task.id)
            steps_completed.append("session_killed")

            if await self.worktrees.get_path(task.id):
                await self.worktrees.delete(task.id, delete_branch=True)
            steps_completed.append("worktree_deleted")

            await self.tasks.delete_task(task.id)
            steps_completed.append("db_deleted")

            log.debug(f"Task {task.id} deleted successfully. Steps: {steps_completed}")
            return True, "Deleted successfully"
        except Exception as e:
            log.error(
                f"Delete failed for task {task.id} after steps: {steps_completed}. Error: {e}"
            )
            return False, f"Delete failed: {e}"

    async def merge_task(self, task: TaskLike) -> tuple[bool, str]:
        """Merge task changes and clean up. Returns (success, message)."""
        config = self.config.general

        async def _do_merge() -> tuple[bool, str]:
            idle, idle_message = await self._ensure_task_idle_before_merge(task.id)
            if not idle:
                assert idle_message is not None
                return False, f"Merge blocked: {idle_message}"

            workspace_id = await self._get_latest_workspace_id(task.id)
            if not workspace_id:
                message = f"Workspace not found for task {task.id}"
                return False, message

            base_branch = (
                getattr(task, "base_branch", None) or self.config.general.default_base_branch
            )
            short_id = task.id.split("-")[0] if "-" in task.id else task.id[:8]
            commit_msg = f"{task.title} (kagan {short_id})"
            if task.description and task.description.strip():
                commit_msg += f"\n\n{task.description}"

            skip_unchanged = await self.has_no_changes(task)
            risk = await self._assess_merge_risk(task, workspace_id, base_branch)
            used_premerge_rebase = False

            if self._should_rebase_before_merge(base_branch, risk):
                (
                    rebase_success,
                    rebase_message,
                    _conflict_files,
                ) = await self.workspace_service.rebase_onto_base(task.id, base_branch)
                if not rebase_success:
                    return False, f"Merge blocked: {rebase_message}"[:500]
                used_premerge_rebase = True

            results = await self.merge_all(
                workspace_id,
                strategy=MergeStrategy.DIRECT,
                skip_unchanged=skip_unchanged,
                commit_message=commit_msg,
            )
            failures = [result for result in results if not result.success]
            used_auto_rebase = False

            if failures and self._should_retry_after_rebase(failures):
                (
                    rebase_success,
                    rebase_message,
                    _conflict_files,
                ) = await self.workspace_service.rebase_onto_base(task.id, base_branch)
                if not rebase_success:
                    return False, f"Merge blocked: {rebase_message}"[:500]

                used_auto_rebase = True
                results = await self.merge_all(
                    workspace_id,
                    strategy=MergeStrategy.DIRECT,
                    skip_unchanged=skip_unchanged,
                    commit_message=commit_msg,
                )
                failures = [result for result in results if not result.success]

            if failures:
                return False, self._summarize_failures(failures, overlap_files=risk.overlap_files)

            await self.worktrees.release(workspace_id, cleanup=False, reason="merged")
            await self.sessions.kill_session(task.id)
            await self.tasks.update_fields(
                task.id,
                status=TaskStatus.DONE,
            )
            if used_premerge_rebase or used_auto_rebase:
                self._note_rebase_hint(base_branch)
            else:
                self._cooldown_rebase_hint(base_branch)

            if used_auto_rebase:
                return True, "Merged all repos (after auto-rebase)"
            if used_premerge_rebase:
                return True, "Merged all repos (after pre-merge rebase)"
            return True, "Merged all repos"

        if config.serialize_merges:
            async with self.automation.merge_lock:
                return await _do_merge()
        return await _do_merge()

    async def close_exploratory(self, task: TaskLike) -> tuple[bool, str]:
        """Close a no-change task by marking DONE and archiving its workspace."""
        if self.automation.is_running(task.id):
            await self.automation.stop_task(task.id)

        await self.sessions.kill_session(task.id)

        workspace_id = await self._get_latest_workspace_id(task.id)
        if workspace_id:
            await self.worktrees.release(workspace_id, cleanup=False, reason="no_changes")

        await self.tasks.update_fields(
            task.id,
            status=TaskStatus.DONE,
        )
        return True, "Closed with no changes"

    async def apply_rejection_feedback(
        self,
        task: TaskLike,
        feedback: str | None,
        action: str = "backlog",
    ) -> Task:
        """Apply rejection feedback and move the task out of REVIEW.

        State Transitions:
            - return: REVIEW → IN_PROGRESS (manual restart)
            - backlog: REVIEW → BACKLOG

        Returns:
            Updated task from database.
        """

        target_status = (
            TaskStatus.BACKLOG if action == RejectionAction.BACKLOG else TaskStatus.IN_PROGRESS
        )

        if feedback:
            timestamp = utc_now().strftime("%Y-%m-%d %H:%M")
            new_description = task.description or ""
            new_description += f"\n\n---\n**Review Feedback ({timestamp}):**\n{feedback}"

            await self.tasks.update_fields(
                task.id,
                description=new_description,
                status=target_status,
            )
        else:
            await self.tasks.update_fields(
                task.id,
                status=target_status,
            )

        refreshed_task = await self.tasks.get_task(task.id)
        assert refreshed_task is not None
        return refreshed_task

    async def has_no_changes(self, task: TaskLike) -> bool:
        """Return True if the task has no commits and no diff stats."""
        workspace_id = await self._get_latest_workspace_id(task.id)
        if not workspace_id:
            return True
        repos = await self.workspace_service.get_workspace_repos(workspace_id)
        if any(repo.get("has_changes") for repo in repos):
            return False

        base_branch = task.base_branch or self.config.general.default_base_branch
        commits = await self.workspace_service.get_commit_log(task.id, base_branch)
        return not commits

    async def merge_repo(
        self,
        workspace_id: str,
        repo_id: str,
        *,
        strategy: MergeStrategy = MergeStrategy.DIRECT,
        pr_title: str | None = None,
        pr_body: str | None = None,
        commit_message: str | None = None,
    ) -> MergeResult:
        """Merge a single repo's changes."""
        if self._events is None or self._git is None:
            raise RuntimeError("Merge service missing dependencies for per-repo operations")

        from sqlmodel import col, select

        from kagan.core.adapters.db.schema import Merge, Repo, Workspace, WorkspaceRepo
        from kagan.core.events import MergeCompleted, MergeFailed, PRCreated

        async with get_required_session(
            self._session_factory,
            error_message="Merge service missing session factory for per-repo operations",
        ) as session:
            result = await session.execute(
                select(WorkspaceRepo, Repo, Workspace)
                .join(Repo, col(WorkspaceRepo.repo_id) == col(Repo.id))
                .join(Workspace, col(WorkspaceRepo.workspace_id) == col(Workspace.id))
                .where(WorkspaceRepo.workspace_id == workspace_id)
                .where(WorkspaceRepo.repo_id == repo_id)
            )
            row = result.first()

        if not row:
            raise ValueError(f"Repo {repo_id} not found in workspace {workspace_id}")

        workspace_repo, repo, workspace = row
        if not workspace_repo.worktree_path:
            raise ValueError(f"Repo {repo_id} has no worktree for workspace {workspace_id}")

        merge_result: MergeResult | None = None
        if await self._git.has_uncommitted_changes(workspace_repo.worktree_path):
            short_id = workspace.task_id[:8] if workspace.task_id else "unknown"
            await self._git.commit_all(
                workspace_repo.worktree_path,
                f"chore: adding uncommitted agent changes ({short_id})",
            )
            log.info(f"Auto-committed changes before merge for repo {repo.name}")

        await self._git.push(workspace_repo.worktree_path, workspace.branch_name)

        if merge_result is None and strategy == MergeStrategy.PULL_REQUEST:
            pr_url = await self._create_pr(
                repo_path=repo.path,
                branch=workspace.branch_name,
                target=workspace_repo.target_branch,
                title=pr_title or f"Merge {workspace.branch_name}",
                body=pr_body or "",
            )
            merge_result = MergeResult(
                repo_id=repo_id,
                repo_name=repo.name,
                strategy=strategy,
                success=True,
                message=f"PR created: {pr_url}",
                pr_url=pr_url,
            )
            await self._events.publish(
                PRCreated(
                    workspace_id=workspace_id,
                    repo_id=repo_id,
                    pr_url=pr_url,
                )
            )
        elif merge_result is None:
            if self._is_remote_target(workspace_repo.target_branch):
                merge_result = MergeResult(
                    repo_id=repo_id,
                    repo_name=repo.name,
                    strategy=strategy,
                    success=False,
                    message=(
                        f"Direct merge blocked for remote target {workspace_repo.target_branch}"
                    ),
                )
                await self._events.publish(
                    MergeFailed(
                        workspace_id=workspace_id,
                        repo_id=repo_id,
                        error=merge_result.message,
                    )
                )
            else:
                try:
                    git_result = await self._git.merge_squash(
                        repo_path=repo.path,
                        source_branch=workspace.branch_name,
                        target_branch=workspace_repo.target_branch,
                        commit_message=commit_message,
                    )
                    if git_result.success:
                        merge_result = MergeResult(
                            repo_id=repo_id,
                            repo_name=repo.name,
                            strategy=strategy,
                            success=True,
                            message=git_result.message,
                            commit_sha=git_result.commit_sha,
                        )
                        if git_result.commit_sha:
                            await self._events.publish(
                                MergeCompleted(
                                    workspace_id=workspace_id,
                                    repo_id=repo_id,
                                    target_branch=workspace_repo.target_branch,
                                    commit_sha=git_result.commit_sha,
                                )
                            )
                    else:
                        merge_result = MergeResult(
                            repo_id=repo_id,
                            repo_name=repo.name,
                            strategy=strategy,
                            success=False,
                            message=git_result.message,
                            conflict_op=git_result.conflict.op if git_result.conflict else None,
                            conflict_files=git_result.conflict.files
                            if git_result.conflict
                            else None,
                        )
                        await self._events.publish(
                            MergeFailed(
                                workspace_id=workspace_id,
                                repo_id=repo_id,
                                error=git_result.message,
                                conflict_op=merge_result.conflict_op,
                                conflict_files=merge_result.conflict_files,
                            )
                        )
                except Exception as exc:
                    merge_result = MergeResult(
                        repo_id=repo_id,
                        repo_name=repo.name,
                        strategy=strategy,
                        success=False,
                        message=str(exc),
                    )
                    await self._events.publish(
                        MergeFailed(
                            workspace_id=workspace_id,
                            repo_id=repo_id,
                            error=str(exc),
                        )
                    )

        async with get_required_session(
            self._session_factory,
            error_message="Merge service missing session factory for per-repo operations",
        ) as session:
            merge_type = (
                MergeType.PR if strategy == MergeStrategy.PULL_REQUEST else MergeType.DIRECT
            )
            merge_record = Merge(
                workspace_id=workspace_id,
                repo_id=repo_id,
                merge_type=merge_type,
                target_branch_name=workspace_repo.target_branch,
                merge_commit=merge_result.commit_sha if merge_type == MergeType.DIRECT else None,
                pr_url=merge_result.pr_url if merge_type == MergeType.PR else None,
                pr_status=(
                    MergeStatus.OPEN
                    if merge_type == MergeType.PR and merge_result.success
                    else MergeStatus.MERGED
                    if merge_result.success
                    else MergeStatus.CLOSED
                ),
                pr_merged_at=utc_now()
                if merge_type == MergeType.PR and merge_result.success
                else None,
                pr_merge_commit_sha=merge_result.commit_sha
                if merge_type == MergeType.PR and merge_result.success
                else None,
            )
            session.add(merge_record)
            await session.commit()

        return merge_result

    async def merge_all(
        self,
        workspace_id: str,
        *,
        strategy: MergeStrategy = MergeStrategy.DIRECT,
        skip_unchanged: bool = True,
        commit_message: str | None = None,
    ) -> list[MergeResult]:
        """Merge all repos in a workspace."""
        repos = await self.workspace_service.get_workspace_repos(workspace_id)
        results: list[MergeResult] = []

        for repo in repos:
            if skip_unchanged and not repo["has_changes"]:
                results.append(
                    MergeResult(
                        repo_id=repo["repo_id"],
                        repo_name=repo["repo_name"],
                        strategy=strategy,
                        success=True,
                        message="Skipped (no changes)",
                    )
                )
                continue
            results.append(
                await self.merge_repo(
                    workspace_id,
                    repo["repo_id"],
                    strategy=strategy,
                    commit_message=commit_message,
                )
            )

        return results

    async def create_pr(
        self,
        workspace_id: str,
        repo_id: str,
        *,
        title: str,
        body: str,
        draft: bool = False,
    ) -> str:
        """Create a pull request for a specific repo."""
        del draft
        result = await self.merge_repo(
            workspace_id,
            repo_id,
            strategy=MergeStrategy.PULL_REQUEST,
            pr_title=title,
            pr_body=body,
        )
        if not result.pr_url:
            raise RuntimeError("PR creation failed")
        return result.pr_url

    async def _create_pr(
        self,
        repo_path: str,
        branch: str,
        target: str,
        title: str,
        body: str,
    ) -> str:
        """Create PR using gh CLI."""
        retry_policy = ProcessRetryPolicy(
            max_attempts=2,
            delay_seconds=0.2,
            retry_on_timeout=True,
            retry_on_nonzero=False,
            retry_on_oserror=True,
        )
        try:
            result = await run_exec_checked(
                "gh",
                "pr",
                "create",
                "--repo",
                repo_path,
                "--head",
                branch,
                "--base",
                target,
                "--title",
                title,
                "--body",
                body,
                retry_policy=retry_policy,
            )
        except ProcessExecutionError as exc:
            raise RuntimeError(f"Failed to create PR: {exc}") from exc
        return result.stdout_text().strip()

    @staticmethod
    def _is_remote_target(target_branch: str) -> bool:
        return target_branch.startswith("origin/") or target_branch.startswith("refs/remotes/")

    @staticmethod
    def _summarize_failures(
        failures: list[MergeResult],
        *,
        overlap_files: tuple[str, ...] = (),
    ) -> str:
        message = "; ".join(f"{result.repo_name}: {result.message}" for result in failures)
        hints: list[str] = []
        has_conflicts = any(
            (result.conflict_files and len(result.conflict_files) > 0)
            or "conflict" in result.message.lower()
            for result in failures
        )
        if has_conflicts:
            hints.append("Tip: run review rebase, resolve conflicts, then merge again")
        if overlap_files:
            preview = ", ".join(overlap_files[:3])
            suffix = "..." if len(overlap_files) > 3 else ""
            hints.append(f"Potential overlap with base changes: {preview}{suffix}")
        if hints:
            message = f"{message}. {' '.join(hints)}"
        return message[:500]

    @staticmethod
    def _should_retry_after_rebase(failures: list[MergeResult]) -> bool:
        return all("rebase required" in result.message.lower() for result in failures)

    async def _assess_merge_risk(
        self,
        task: TaskLike,
        workspace_id: str,
        base_branch: str,
    ) -> MergeRisk:
        repos = await self.workspace_service.get_workspace_repos(workspace_id)
        changed_repo_count = sum(1 for repo in repos if repo.get("has_changes"))
        commits = await self.workspace_service.get_commit_log(task.id, base_branch)
        changed_files = await self.workspace_service.get_files_changed(task.id, base_branch)
        base_changed_files = await self.workspace_service.get_files_changed_on_base(
            task.id, base_branch
        )
        overlap_files = tuple(sorted(set(changed_files).intersection(base_changed_files)))

        score = 0
        if changed_repo_count > 1:
            score += 1
        if len(commits) >= 6:
            score += 1
        if len(changed_files) >= 12:
            score += 1
        if overlap_files:
            score += 2

        return MergeRisk(
            score=score,
            overlap_files=overlap_files,
            commit_count=len(commits),
            changed_repo_count=changed_repo_count,
            changed_file_count=len(changed_files),
        )

    def _should_rebase_before_merge(self, base_branch: str, risk: MergeRisk) -> bool:
        if self._rebase_first_hints.get(base_branch, 0) > 0:
            return True
        return risk.high

    def _note_rebase_hint(self, base_branch: str) -> None:
        self._rebase_first_hints[base_branch] = min(
            self._rebase_first_hints.get(base_branch, 0) + 1,
            3,
        )

    def _cooldown_rebase_hint(self, base_branch: str) -> None:
        hint = self._rebase_first_hints.get(base_branch, 0)
        if hint <= 1:
            self._rebase_first_hints.pop(base_branch, None)
            return
        self._rebase_first_hints[base_branch] = hint - 1
