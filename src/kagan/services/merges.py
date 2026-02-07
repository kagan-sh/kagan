"""Merge service operations - decoupled from UI."""

from __future__ import annotations

import logging
from dataclasses import dataclass
from enum import Enum
from typing import TYPE_CHECKING, Protocol

from kagan.core.models.enums import MergeReadiness, TaskStatus

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

    from kagan.adapters.git.operations import GitOperationsAdapter
    from kagan.config import KaganConfig
    from kagan.core.events import EventBus
    from kagan.core.models.entities import Task
    from kagan.services.automation import AutomationServiceImpl
    from kagan.services.sessions import SessionService
    from kagan.services.tasks import TaskService
    from kagan.services.types import TaskLike
    from kagan.services.workspaces import WorkspaceService

log = logging.getLogger(__name__)


class MergeStrategy(str, Enum):
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


class MergeService(Protocol):
    """Service interface for merge operations."""

    async def delete_task(self, task: TaskLike) -> tuple[bool, str]: ...

    async def merge_task(self, task: TaskLike) -> tuple[bool, str]: ...

    async def close_exploratory(self, task: TaskLike) -> tuple[bool, str]: ...

    async def apply_rejection_feedback(
        self,
        task: TaskLike,
        feedback: str | None,
        action: str = "shelve",
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
    ) -> MergeResult: ...

    async def merge_all(
        self,
        workspace_id: str,
        *,
        strategy: MergeStrategy = MergeStrategy.DIRECT,
        skip_unchanged: bool = True,
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

    def __init__(
        self,
        task_service: TaskService,
        worktrees: WorkspaceService,
        sessions: SessionService,
        automation: AutomationServiceImpl,
        config: KaganConfig,
        session_factory: async_sessionmaker[AsyncSession] | None = None,
        event_bus: EventBus | None = None,
        git_adapter: GitOperationsAdapter | None = None,
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

    def _get_session(self) -> AsyncSession:
        if self._session_factory is None:
            raise RuntimeError("Merge service missing session factory for per-repo operations")
        return self._session_factory()

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
            # Step 1: Stop agent if running
            if self.automation.is_running(task.id):
                await self.automation.stop_task(task.id)
            steps_completed.append("agent_stopped")

            # Step 2: Kill session
            await self.sessions.kill_session(task.id)
            steps_completed.append("session_killed")

            # Step 3: Delete worktree
            if await self.worktrees.get_path(task.id):
                await self.worktrees.delete(task.id, delete_branch=True)
            steps_completed.append("worktree_deleted")

            # Step 4: Delete from database (point of no return)
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
        base = self.config.general.default_base_branch
        config = self.config.general

        if config.require_review_approval and task.checks_passed is not True:
            message = "Review approval required before merge."
            await self.tasks.update_fields(
                task.id,
                merge_failed=True,
                merge_error=message,
                merge_readiness=MergeReadiness.BLOCKED,
            )
            await self.tasks.append_event(task.id, "policy", message)
            return False, message

        async def _do_merge() -> tuple[bool, str]:
            await self.tasks.update_fields(
                task.id,
                merge_failed=False,
                merge_error=None,
                merge_readiness=MergeReadiness.RISK,
            )

            workspace_id = await self._get_latest_workspace_id(task.id)
            if not workspace_id:
                message = f"Workspace not found for task {task.id}"
                await self.tasks.update_fields(
                    task.id,
                    merge_failed=True,
                    merge_error=message[:500],
                    merge_readiness=MergeReadiness.BLOCKED,
                )
                await self.tasks.append_event(task.id, "merge", message)
                return False, message

            results = await self.merge_all(
                workspace_id,
                strategy=MergeStrategy.DIRECT,
                skip_unchanged=True,
            )
            failures = [result for result in results if not result.success]
            if failures:
                message = "; ".join(f"{result.repo_name}: {result.message}" for result in failures)[
                    :500
                ]
                await self.tasks.update_fields(
                    task.id,
                    merge_failed=True,
                    merge_error=message,
                    merge_readiness=MergeReadiness.BLOCKED,
                )
                await self.tasks.append_event(task.id, "merge", f"Merge failed: {message}")
                return False, message

            await self.worktrees.delete(task.id, delete_branch=True)
            await self.sessions.kill_session(task.id)
            await self.tasks.update_fields(
                task.id,
                status=TaskStatus.DONE,
                merge_failed=False,
                merge_error=None,
                merge_readiness=MergeReadiness.READY,
            )
            await self.tasks.append_event(task.id, "merge", f"Merged to {base}")
            return True, "Merged all repos"

        if config.serialize_merges:
            async with self.automation.merge_lock:
                return await _do_merge()
        return await _do_merge()

    async def close_exploratory(self, task: TaskLike) -> tuple[bool, str]:
        """Close a DONE task by deleting it (used for no-change exploratory tasks)."""
        if await self.worktrees.get_path(task.id):
            await self.worktrees.delete(task.id, delete_branch=True)
        await self.sessions.kill_session(task.id)

        # Stop agent if running
        if self.automation.is_running(task.id):
            await self.automation.stop_task(task.id)

        # Delete task (exploratory tasks are removed, not kept as DONE)
        await self.tasks.delete_task(task.id)
        return True, "Closed as exploratory"

    async def apply_rejection_feedback(
        self,
        task: TaskLike,
        feedback: str | None,
        action: str = "shelve",  # "retry" | "stage" | "shelve"
    ) -> Task:
        """Apply rejection feedback with state transition per Active Iteration Model.

        State Transitions:
            - retry: REVIEW → IN_PROGRESS (agent spawned, iterations reset)
            - stage: REVIEW → IN_PROGRESS (agent paused, iterations reset)
            - shelve: REVIEW → BACKLOG (iterations preserved)

        Returns:
            Updated task from database.
        """
        # Determine target status based on action
        target_status = TaskStatus.BACKLOG if action == "shelve" else TaskStatus.IN_PROGRESS

        # Append feedback to description if provided
        if feedback:
            from datetime import datetime

            timestamp = datetime.now().strftime("%Y-%m-%d %H:%M")
            new_description = task.description or ""
            new_description += f"\n\n---\n**Review Feedback ({timestamp}):**\n{feedback}"

            await self.tasks.update_fields(
                task.id,
                description=new_description,
                status=target_status,
                merge_failed=False,
                merge_error=None,
                merge_readiness=MergeReadiness.RISK,
            )
            await self.tasks.append_event(
                task.id, "review", f"Rejected with feedback: {feedback[:200]}"
            )
        else:
            await self.tasks.update_fields(
                task.id,
                status=target_status,
                merge_failed=False,
                merge_error=None,
                merge_readiness=MergeReadiness.RISK,
            )
            await self.tasks.append_event(task.id, "review", "Rejected")

        # Reset iterations for retry/stage actions (not shelve)
        if action in ("retry", "stage"):
            self.automation.reset_iterations(task.id)

        # Return refreshed task
        refreshed_task = await self.tasks.get_task(task.id)
        assert refreshed_task is not None
        return refreshed_task

    async def has_no_changes(self, task: TaskLike) -> bool:
        """Return True if the task has no commits and no diff stats."""
        workspace_id = await self._get_latest_workspace_id(task.id)
        if not workspace_id:
            return True
        repos = await self.workspace_service.get_workspace_repos(workspace_id)
        return not any(repo.get("has_changes") for repo in repos)

    async def merge_repo(
        self,
        workspace_id: str,
        repo_id: str,
        *,
        strategy: MergeStrategy = MergeStrategy.DIRECT,
        pr_title: str | None = None,
        pr_body: str | None = None,
    ) -> MergeResult:
        """Merge a single repo's changes."""
        if self._events is None or self._git is None:
            raise RuntimeError("Merge service missing dependencies for per-repo operations")

        from datetime import datetime

        from sqlmodel import col, select

        from kagan.adapters.db.schema import Merge, Repo, Workspace, WorkspaceRepo
        from kagan.core.events import MergeCompleted, MergeFailed, PRCreated
        from kagan.core.models.enums import MergeStatus

        async with self._get_session() as session:
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

        if await self._git.has_uncommitted_changes(workspace_repo.worktree_path):
            await self._git.commit_all(
                workspace_repo.worktree_path,
                message="Auto-commit before merge",
            )

        await self._git.push(workspace_repo.worktree_path, workspace.branch_name)

        if strategy == MergeStrategy.PULL_REQUEST:
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
        else:
            try:
                commit_sha = await self._git.merge_branch(
                    repo_path=repo.path,
                    source_branch=workspace.branch_name,
                    target_branch=workspace_repo.target_branch,
                )
                merge_result = MergeResult(
                    repo_id=repo_id,
                    repo_name=repo.name,
                    strategy=strategy,
                    success=True,
                    message=f"Merged to {workspace_repo.target_branch}",
                    commit_sha=commit_sha,
                )
                await self._events.publish(
                    MergeCompleted(
                        workspace_id=workspace_id,
                        repo_id=repo_id,
                        target_branch=workspace_repo.target_branch,
                        commit_sha=commit_sha,
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

        if workspace.task_id:
            async with self._get_session() as session:
                merge_record = Merge(
                    task_id=workspace.task_id,
                    workspace_id=workspace_id,
                    repo_id=repo_id,
                    strategy=strategy.value,
                    target_branch=workspace_repo.target_branch,
                    commit_sha=merge_result.commit_sha,
                    status=MergeStatus.MERGED if merge_result.success else MergeStatus.FAILED,
                    pr_url=merge_result.pr_url,
                    error=None if merge_result.success else merge_result.message,
                    merged_at=datetime.now() if merge_result.success else None,
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
        import asyncio

        proc = await asyncio.create_subprocess_exec(
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
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await proc.communicate()

        if proc.returncode != 0:
            raise RuntimeError(f"Failed to create PR: {stderr.decode()}")

        return stdout.decode().strip()
