"""Automation, session, job, and runtime API mixin.

Contains all automation/agent lifecycle, session management, job operations,
execution queries, runtime state, workspace operations, merge operations,
diff, planner, agent health, and service property accessors.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

from kagan.core.models.enums import TaskType

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

    from kagan.core.adapters.db.schema import Task
    from kagan.core.bootstrap import AppContext
    from kagan.core.config import KaganConfig
    from kagan.core.services.jobs import JobEvent, JobRecord
    from kagan.core.services.merges import MergeResult, MergeStrategy
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

    async def reconcile_running_tasks(self, task_ids: Sequence[str]) -> None:
        """Synchronize runtime task projections with database state."""
        await self._ctx.runtime_service.reconcile_running_tasks(task_ids)

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
        return await self._ctx.diff_service.get_all_diffs(workspace_id)

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

    async def cleanup_orphan_workspaces(self, valid_task_ids: set[str]) -> list[str]:
        """Clean up workspaces whose tasks no longer exist."""
        return await self._ctx.workspace_service.cleanup_orphans(valid_task_ids)

    async def get_workspace_diff(self, task_id: str, *, base_branch: str) -> str:
        """Get the diff for a task's workspace against a base branch."""
        return await self._ctx.workspace_service.get_diff(task_id, base_branch)

    async def rebase_workspace(self, task_id: str, base_branch: str) -> tuple[bool, str, list[str]]:
        """Rebase a task's workspace onto a base branch."""
        return await self._ctx.workspace_service.rebase_onto_base(task_id, base_branch)

    async def abort_workspace_rebase(self, task_id: str) -> None:
        """Abort an in-progress rebase for a task's workspace."""
        await self._ctx.workspace_service.abort_rebase(task_id)

    # ── Special Bootstrap & Accessors ─────────────────────────────────

    def bootstrap_session_service(self, project_root: Path, config: KaganConfig) -> None:
        """Bootstrap the session service with runtime dependencies."""
        from kagan.core.services.sessions import SessionServiceImpl

        self._ctx.session_service = SessionServiceImpl(
            project_root,
            self._ctx.task_service,
            self._ctx.workspace_service,
            config,
        )

    # ── Service Property Accessors ────────────────────────────────────

    @property
    def workspace_service(self) -> Any:
        """Direct access to the workspace service for widget constructors."""
        return self._ctx.workspace_service

    @property
    def diff_service(self) -> Any:
        """Direct access to the diff service, or None if unavailable."""
        return getattr(self._ctx, "diff_service", None)

    @property
    def execution_repo(self) -> Any:
        """Direct access to the execution repository."""
        return self._ctx.execution_service
