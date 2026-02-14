"""Automation, session, job, and runtime API mixin.

Contains all automation/agent lifecycle, session management, job operations,
execution queries, runtime state, workspace operations, merge operations,
diff, planner, and agent health.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

from kagan.core.models.enums import TaskType
from kagan.core.runtime_helpers import runtime_snapshot_for_task

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

    from kagan.core.adapters.db.schema import Task
    from kagan.core.bootstrap import AppContext
    from kagan.core.services.jobs import JobEvent, JobRecord
    from kagan.core.services.merges import MergeResult, MergeStrategy
    from kagan.core.services.queued_messages import QueuedMessage, QueueLane, QueueStatus
    from kagan.core.services.runtime import (
        AutoOutputReadiness,
        RuntimeContextState,
        RuntimeSessionEvent,
    )
    from kagan.core.services.workspaces import RepoWorkspaceInput

logger = logging.getLogger(__name__)


class AutomationApiMixin:
    """Mixin providing automation, session, job, and runtime API methods.

    Expects ``self._ctx`` to be an :class:`AppContext` instance,
    initialised by :class:`KaganAPI.__init__`.
    """

    _ctx: AppContext

    # ── Jobs ───────────────────────────────────────────────────────────

    async def submit_job(
        self,
        task_id: str,
        action: str,
        *,
        arguments: dict[str, Any] | None = None,
    ) -> JobRecord:
        """Submit an asynchronous job for a task."""
        payload: dict[str, Any] = {"task_id": task_id}
        if arguments:
            payload.update(arguments)
        return await self._ctx.job_service.submit(task_id=task_id, action=action, params=payload)

    async def cancel_job(self, job_id: str, *, task_id: str) -> JobRecord | None:
        """Cancel a submitted job."""
        return await self._ctx.job_service.cancel(job_id, task_id=task_id)

    async def get_job(self, job_id: str, *, task_id: str | None = None) -> JobRecord | None:
        """Get job details, optionally verifying task ownership."""
        job = await self._ctx.job_service.get(job_id)
        if job is None:
            return None
        if task_id is not None and job.task_id != task_id:
            return None
        return job

    async def wait_job(
        self,
        job_id: str,
        *,
        task_id: str,
        timeout_seconds: float | None = None,
    ) -> JobRecord | None:
        """Wait for a job to reach terminal status."""
        return await self._ctx.job_service.wait(
            job_id, task_id=task_id, timeout_seconds=timeout_seconds
        )

    async def get_job_events(self, job_id: str, *, task_id: str) -> list[JobEvent] | None:
        """List events emitted by a submitted job."""
        return await self._ctx.job_service.events(job_id, task_id=task_id)

    # ── Sessions ───────────────────────────────────────────────────────

    async def create_session(
        self,
        task_id: str,
        *,
        worktree_path: Path | None = None,
        reuse_if_exists: bool = True,
    ) -> Any:
        """Create a PAIR session for a task.

        Returns:
            SessionCreateResult with session_name, already_exists, worktree_path, task.

        Raises:
            TaskNotFoundError: task does not exist.
            TaskTypeMismatchError: task is not PAIR type.
            WorkspaceNotFoundError: no workspace provisioned.
            InvalidWorktreePathError: provided path doesn't match expected.
            SessionCreateFailedError: backend failed to create session.
        """
        from kagan.core.api import (
            InvalidWorktreePathError,
            SessionCreateFailedError,
            SessionCreateResult,
            TaskNotFoundError,
            TaskTypeMismatchError,
            WorkspaceNotFoundError,
        )

        task = await self._ctx.task_service.get_task(task_id)
        if task is None:
            raise TaskNotFoundError(task_id)

        if task.task_type != TaskType.PAIR:
            raise TaskTypeMismatchError(task_id, task.task_type.value)

        expected_worktree = await self._ctx.workspace_service.get_path(task_id)
        if expected_worktree is None:
            raise WorkspaceNotFoundError(task_id)

        resolved_worktree = expected_worktree
        if worktree_path is not None:
            provided = worktree_path.expanduser().resolve(strict=False)
            expected_resolved = expected_worktree.resolve(strict=False)
            if provided != expected_resolved:
                raise InvalidWorktreePathError(
                    task_id,
                    f"worktree_path must point to the task workspace. "
                    f"Expected: {expected_resolved}",
                )
            resolved_worktree = expected_resolved

        already_exists = await self._ctx.session_service.session_exists(task_id)
        if already_exists and not reuse_if_exists:
            await self._ctx.session_service.kill_session(task_id)
            already_exists = False

        try:
            session_name = (
                f"kagan-{task_id}"
                if already_exists
                else await self._ctx.session_service.create_session(task, resolved_worktree)
            )
        except Exception as exc:  # quality-allow-broad-except
            raise SessionCreateFailedError(task_id, exc) from exc

        return SessionCreateResult(
            session_name=session_name,
            already_exists=already_exists,
            worktree_path=resolved_worktree,
            task=task,
        )

    async def attach_session(self, task_id: str) -> bool:
        """Attach to an existing PAIR session."""
        return await self._ctx.session_service.attach_session(task_id)

    async def session_exists(self, task_id: str) -> bool:
        """Check if a session exists for a task."""
        return await self._ctx.session_service.session_exists(task_id)

    async def kill_session(self, task_id: str) -> None:
        """Kill a PAIR session."""
        await self._ctx.session_service.kill_session(task_id)

    # ── Automation Operations ─────────────────────────────────────────

    def is_automation_running(self, task_id: str) -> bool:
        """Check if automation is running for a task (sync)."""
        return self._ctx.automation_service.is_running(task_id)

    def get_running_agent(self, task_id: str) -> Any:
        """Get the running agent for a task (sync)."""
        return self._ctx.automation_service.get_running_agent(task_id)

    async def wait_for_running_agent(self, task_id: str, *, timeout: float = 2.0) -> Any:
        """Wait for a running agent to attach for a task."""
        return await self._ctx.automation_service.wait_for_running_agent(task_id, timeout=timeout)

    async def start_automation(self) -> None:
        """Start the automation service."""
        await self._ctx.automation_service.start()

    async def queue_message(
        self,
        session_id: str,
        content: str,
        *,
        lane: QueueLane = "implementation",
        author: str | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> QueuedMessage:
        """Queue a follow-up message for implementation/review/planner lanes."""
        return await self._ctx.automation_service.queue_message(
            session_id,
            content,
            lane=lane,
            author=author,
            metadata=metadata,
        )

    async def get_queue_status(
        self,
        session_id: str,
        *,
        lane: QueueLane = "implementation",
    ) -> QueueStatus:
        """Get queue status for a specific lane."""
        return await self._ctx.automation_service.get_status(session_id, lane=lane)

    async def get_queued_messages(
        self,
        session_id: str,
        *,
        lane: QueueLane = "implementation",
    ) -> list[QueuedMessage]:
        """List queued messages without consuming them."""
        return await self._ctx.automation_service.get_queued(session_id, lane=lane)

    async def take_queued_message(
        self,
        session_id: str,
        *,
        lane: QueueLane = "implementation",
    ) -> QueuedMessage | None:
        """Consume and return the next queued message payload for a lane."""
        return await self._ctx.automation_service.take_queued(session_id, lane=lane)

    async def remove_queued_message(
        self,
        session_id: str,
        index: int,
        *,
        lane: QueueLane = "implementation",
    ) -> bool:
        """Remove a queued message by index from a lane."""
        return await self._ctx.automation_service.remove_message(session_id, index, lane=lane)

    # ── Execution Operations ──────────────────────────────────────────

    async def get_execution_logs(self, execution_id: str) -> Any:
        """Return aggregated execution logs for an execution."""
        return await self._ctx.execution_service.get_execution_logs(execution_id)

    async def get_execution(self, execution_id: str) -> Any:
        """Return execution record by ID."""
        return await self._ctx.execution_service.get_execution(execution_id)

    async def get_execution_log_entries(self, execution_id: str) -> list[Any]:
        """Return ordered execution log entries for an execution."""
        return await self._ctx.execution_service.get_execution_log_entries(execution_id)

    async def get_latest_execution_for_task(self, task_id: str) -> Any:
        """Return most recent execution for a task."""
        return await self._ctx.execution_service.get_latest_execution_for_task(task_id)

    async def count_executions_for_task(self, task_id: str) -> int:
        """Return total executions for a task."""
        return await self._ctx.execution_service.count_executions_for_task(task_id)

    # ── Runtime Operations ────────────────────────────────────────────

    def get_runtime_view(self, task_id: str) -> Any:
        """Get the RuntimeTaskView for a task (in-memory projection)."""
        return self._ctx.runtime_service.get(task_id)

    def get_running_task_ids(self) -> set[str]:
        """Return the set of currently running task IDs."""
        return self._ctx.runtime_service.running_tasks()

    async def reconcile_running_tasks(self, task_ids: Sequence[str]) -> list[dict[str, Any]]:
        """Synchronize runtime task projections and return refreshed runtime snapshots."""
        unique_task_ids = tuple(dict.fromkeys(task_ids))
        if not unique_task_ids:
            return []

        await self._ctx.runtime_service.reconcile_running_tasks(unique_task_ids)
        return [
            {
                "task_id": task_id,
                "runtime": runtime_snapshot_for_task(
                    task_id=task_id,
                    runtime_service=self._ctx.runtime_service,
                ),
            }
            for task_id in unique_task_ids
        ]

    async def decide_startup(self, cwd: Path) -> Any:
        """Determine startup flow based on persisted runtime state and cwd."""
        return await self._ctx.runtime_service.decide_startup(cwd)

    async def dispatch_runtime_session(
        self,
        event: RuntimeSessionEvent,
        *,
        project_id: str | None = None,
        repo_id: str | None = None,
    ) -> RuntimeContextState:
        """Dispatch a runtime session event."""
        return await self._ctx.runtime_service.dispatch(
            event, project_id=project_id, repo_id=repo_id
        )

    @property
    def runtime_state(self) -> Any:
        """Access the current runtime session state."""
        return self._ctx.runtime_service.state

    async def prepare_auto_output(self, task: Task) -> AutoOutputReadiness:
        """Prepare AUTO output modal readiness for a task."""
        return await self._ctx.runtime_service.prepare_auto_output(task)

    async def recover_stale_auto_output(self, task: Task) -> Any:
        """Recover stale AUTO output for a task."""
        return await self._ctx.runtime_service.recover_stale_auto_output(task)

    # ── Agent health ──────────────────────────────────────────────────

    def refresh_agent_health(self) -> None:
        """Refresh agent health status."""
        self._ctx.agent_health.refresh()

    def is_agent_available(self) -> bool:
        """Check if the configured agent is available."""
        return self._ctx.agent_health.is_available()

    def get_agent_status_message(self) -> str | None:
        """Get a human-readable agent status message."""
        return self._ctx.agent_health.get_status_message()

    # ── Diffs ─────────────────────────────────────────────────────────

    async def get_all_diffs(self, workspace_id: str) -> Any:
        """Retrieve all diffs for a workspace during task review."""
        service = getattr(self._ctx, "diff_service", None)
        if service is None:
            raise RuntimeError("Diff service unavailable")
        return await service.get_all_diffs(workspace_id)

    # ── Planner ───────────────────────────────────────────────────────

    async def save_plan_proposal(self, proposal: Any) -> Any:
        """Save a planner proposal."""
        repo = getattr(self._ctx, "planner_repository", None)
        if repo is None:
            raise RuntimeError("Planner repository not available")
        return await repo.save(proposal)

    async def get_plan_proposal(self, task_id: str) -> Any:
        """Get the latest planner proposal for a task."""
        repo = getattr(self._ctx, "planner_repository", None)
        if repo is None:
            return None
        return await repo.get_latest(task_id)

    async def save_planner_draft(
        self,
        *,
        project_id: str,
        repo_id: str | None = None,
        tasks_json: list[dict[str, Any]],
        todos_json: list[dict[str, Any]] | None = None,
    ) -> Any | None:
        """Persist a planner draft proposal."""
        repo = getattr(self._ctx, "planner_repository", None)
        if repo is None:
            return None
        return await repo.save_proposal(
            project_id=project_id,
            repo_id=repo_id,
            tasks_json=tasks_json,
            todos_json=todos_json,
        )

    async def list_pending_planner_drafts(
        self,
        project_id: str,
        *,
        repo_id: str | None = None,
    ) -> list[Any]:
        """List pending planner draft proposals for a project/repo scope."""
        repo = getattr(self._ctx, "planner_repository", None)
        if repo is None:
            return []
        return await repo.list_pending(project_id, repo_id=repo_id)

    async def update_planner_draft_status(self, proposal_id: str, status: Any) -> Any | None:
        """Update planner draft status (approved/rejected)."""
        repo = getattr(self._ctx, "planner_repository", None)
        if repo is None:
            return None
        return await repo.update_status(proposal_id, status)

    # ── Merge Operations ────────────────────────────────────────────────

    async def has_no_changes(self, task: Task) -> bool:
        """Check if a task has no uncommitted changes or new commits."""
        svc = getattr(self._ctx, "merge_service", None)
        if svc is None:
            return False
        return await svc.has_no_changes(task)

    async def close_exploratory(self, task: Task) -> tuple[bool, str]:
        """Close a no-change task by marking DONE and archiving its workspace."""
        svc = getattr(self._ctx, "merge_service", None)
        if svc is None:
            return (False, "Merge service unavailable")
        return await svc.close_exploratory(task)

    async def merge_repo(
        self,
        workspace_id: str,
        repo_id: str,
        *,
        strategy: MergeStrategy,
        pr_title: str | None = None,
        pr_body: str | None = None,
        commit_message: str | None = None,
    ) -> MergeResult:
        """Merge a single repo's changes."""
        return await self._ctx.merge_service.merge_repo(
            workspace_id,
            repo_id,
            strategy=strategy,
            pr_title=pr_title,
            pr_body=pr_body,
            commit_message=commit_message,
        )

    async def apply_rejection_feedback(
        self, task: Task, feedback: str | None, action: str
    ) -> Task | None:
        """Apply rejection feedback and move a task out of REVIEW."""
        svc = getattr(self._ctx, "merge_service", None)
        if svc is None:
            return None
        return await svc.apply_rejection_feedback(task, feedback, action)

    async def merge_task_direct(self, task: Task) -> tuple[bool, str]:
        """Merge task changes directly (bypassing review-gate logic)."""
        svc = getattr(self._ctx, "merge_service", None)
        if svc is None:
            return (False, "Merge service unavailable")
        return await svc.merge_task(task)

    # ── Workspace Operations ──────────────────────────────────────────

    async def get_workspace_path(self, task_id: str) -> Path | None:
        """Get the filesystem path for a task's workspace."""
        return await self._ctx.workspace_service.get_path(task_id)

    async def provision_workspace(self, *, task_id: str, repos: list[RepoWorkspaceInput]) -> str:
        """Provision a workspace with worktrees for all repos."""
        return await self._ctx.workspace_service.provision(task_id, repos)

    async def list_workspaces(self, *, task_id: str | None = None) -> list[Any]:
        """List workspaces, optionally filtered by task."""
        return await self._ctx.workspace_service.list_workspaces(task_id=task_id)

    async def get_workspace_repos(self, workspace_id: str) -> list[dict[str, Any]]:
        """List repository records for a workspace."""
        return await self._ctx.workspace_service.get_workspace_repos(workspace_id)

    async def cleanup_orphan_workspaces(self, valid_task_ids: set[str]) -> list[str]:
        """Clean up workspaces whose tasks no longer exist."""
        return await self._ctx.workspace_service.cleanup_orphans(valid_task_ids)

    async def run_workspace_janitor(
        self,
        valid_workspace_ids: set[str],
        *,
        prune_worktrees: bool = True,
        gc_branches: bool = True,
    ) -> Any:
        """Run janitor cleanup for stale worktrees and orphan kagan/* branches.

        This performs two cleanup operations:
        1. Worktree pruning: Runs `git worktree prune` on all project repos.
        2. Branch GC: Deletes orphaned `kagan/*` branches not in valid_workspace_ids.

        Returns:
            JanitorResult with worktrees_pruned, branches_deleted, repos_processed.
        """
        return await self._ctx.workspace_service.run_janitor(
            valid_workspace_ids,
            prune_worktrees=prune_worktrees,
            gc_branches=gc_branches,
        )

    async def get_workspace_diff(self, task_id: str, *, base_branch: str) -> str:
        """Get the diff for a task's workspace against a base branch."""
        return await self._ctx.workspace_service.get_diff(task_id, base_branch)

    async def get_workspace_commit_log(self, task_id: str, *, base_branch: str) -> list[str]:
        """Get commit log for a task workspace against a base branch."""
        return await self._ctx.workspace_service.get_commit_log(task_id, base_branch)

    async def get_workspace_diff_stats(self, task_id: str, *, base_branch: str) -> str:
        """Get summarized diff stats for a task workspace against a base branch."""
        return await self._ctx.workspace_service.get_diff_stats(task_id, base_branch)

    async def get_repo_diff(self, workspace_id: str, repo_id: str) -> Any:
        """Get diff details for one repository in a workspace."""
        return await self._ctx.diff_service.get_repo_diff(workspace_id, repo_id)

    async def rebase_workspace(self, task_id: str, base_branch: str) -> tuple[bool, str, list[str]]:
        """Rebase a task's workspace onto a base branch."""
        return await self._ctx.workspace_service.rebase_onto_base(task_id, base_branch)

    async def abort_workspace_rebase(self, task_id: str) -> None:
        """Abort an in-progress rebase for a task's workspace."""
        await self._ctx.workspace_service.abort_rebase(task_id)
