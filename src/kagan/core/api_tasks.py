"""Task-related API mixin.

Contains all task CRUD, scratchpad, context, logs, search, and review methods.
"""

from __future__ import annotations

import contextlib
import logging
from typing import TYPE_CHECKING, Any

from kagan.core.expose import expose
from kagan.core.models.enums import TaskStatus, TaskType

if TYPE_CHECKING:
    from collections.abc import Sequence

    from kagan.core.adapters.db.schema import Task
    from kagan.core.bootstrap import AppContext
    from kagan.core.models.enums import PairTerminalBackend, TaskPriority

logger = logging.getLogger(__name__)


class TaskApiMixin:
    """Mixin providing task-related API methods.

    Expects ``self._ctx`` to be an :class:`AppContext` instance,
    initialised by :class:`KaganAPI.__init__`.
    """

    _ctx: AppContext

    # ── Tasks ──────────────────────────────────────────────────────────

    @expose("tasks", "create", profile="operator", mutating=True, description="Create a new task.")
    async def create_task(
        self,
        title: str,
        description: str = "",
        *,
        project_id: str | None = None,
        created_by: str | None = None,
        status: TaskStatus | None = None,
        priority: TaskPriority | None = None,
        task_type: TaskType | None = None,
        terminal_backend: PairTerminalBackend | None = None,
        agent_backend: str | None = None,
        parent_id: str | None = None,
        base_branch: str | None = None,
        acceptance_criteria: list[str] | None = None,
    ) -> Task:
        """Create a new task with optional field overrides."""
        task = await self._ctx.task_service.create_task(
            title=title,
            description=description,
            project_id=project_id,
            created_by=created_by,
        )

        fields: dict[str, object] = {}
        if status is not None:
            fields["status"] = status
        if priority is not None:
            fields["priority"] = priority
        if task_type is not None:
            fields["task_type"] = task_type
        if terminal_backend is not None:
            fields["terminal_backend"] = terminal_backend
        if agent_backend is not None:
            fields["agent_backend"] = agent_backend
        if parent_id is not None:
            fields["parent_id"] = parent_id
        if base_branch is not None:
            fields["base_branch"] = base_branch
        if acceptance_criteria is not None:
            fields["acceptance_criteria"] = acceptance_criteria

        if fields:
            updated = await self._ctx.task_service.update_fields(task.id, **fields)
            if updated is not None:
                task = updated

        return task

    @expose("tasks", "get", description="Get a single task by ID.")
    async def get_task(self, task_id: str) -> Task | None:
        """Get a single task by ID."""
        return await self._ctx.task_service.get_task(task_id)

    @expose("tasks", "list", description="List tasks with optional project/status filter.")
    async def list_tasks(
        self,
        *,
        project_id: str | None = None,
        status: TaskStatus | None = None,
    ) -> list[Task]:
        """List tasks with optional project/status filter."""
        return await self._ctx.task_service.list_tasks(project_id=project_id, status=status)

    @expose("tasks", "update", profile="operator", mutating=True, description="Update task fields.")
    async def update_task(
        self,
        task_id: str,
        **fields: object,
    ) -> Task | None:
        """Update task fields.

        Handles task_type transitions (killing sessions or stopping agents)
        when switching between PAIR and AUTO.
        """
        current = await self._ctx.task_service.get_task(task_id)
        if current is None:
            return None

        new_task_type = fields.get("task_type")
        if isinstance(new_task_type, TaskType) and new_task_type != current.task_type:
            await self._handle_task_type_transition(
                task_id=task_id,
                current_type=current.task_type,
                new_type=new_task_type,
                fields=fields,
            )

        return await self._ctx.task_service.update_fields(task_id, **fields)

    @expose(
        "tasks",
        "move",
        profile="operator",
        mutating=True,
        description="Move a task to a new status column.",
    )
    async def move_task(self, task_id: str, status: TaskStatus) -> Task | None:
        """Move a task to a new status column."""
        return await self._ctx.task_service.move(task_id, status)

    @expose(
        "tasks",
        "delete",
        profile="maintainer",
        mutating=True,
        description="Delete a task.",
    )
    async def delete_task(self, task_id: str) -> tuple[bool, str]:
        """Delete a task, coordinating across services.

        Returns:
            Tuple of (success, message).
        """
        task = await self._ctx.task_service.get_task(task_id)
        if task is None:
            return False, f"Task {task_id} not found"

        merge_service = getattr(self._ctx, "merge_service", None)
        if merge_service is not None:
            return await merge_service.delete_task(task)

        if self._ctx.automation_service.is_running(task_id):
            await self._ctx.automation_service.stop_task(task_id)
        if await self._ctx.session_service.session_exists(task_id):
            await self._ctx.session_service.kill_session(task_id)
        if await self._ctx.workspace_service.get_path(task_id):
            await self._ctx.workspace_service.delete(task_id, delete_branch=True)

        deleted = await self._ctx.task_service.delete_task(task_id)
        message = "Deleted successfully" if deleted else f"Task {task_id} not found"
        return deleted, message

    @expose(
        "tasks",
        "update_scratchpad",
        profile="pair_worker",
        mutating=True,
        description="Append to task scratchpad.",
    )
    async def update_scratchpad(self, task_id: str, content: str) -> None:
        """Append content to a task's scratchpad."""
        existing = await self._ctx.task_service.get_scratchpad(task_id)
        updated = f"{existing}\n{content}".strip() if existing else content
        await self._ctx.task_service.update_scratchpad(task_id, updated)

    @expose("tasks", "scratchpad", description="Get a task's scratchpad content.")
    async def get_scratchpad(self, task_id: str) -> str:
        """Get a task's scratchpad content."""
        return await self._ctx.task_service.get_scratchpad(task_id)

    @expose("tasks", "context", description="Get task context for AI tools.")
    async def get_task_context(self, task_id: str) -> dict[str, Any]:
        """Return expanded task context for coordination and implementation.

        Collects task details, scratchpad, linked tasks, workspace info,
        and repository metadata.
        """
        task = await self._ctx.task_service.get_task(task_id)
        if task is None:
            return {"found": False, "task": None}

        scratchpad = await self._ctx.task_service.get_scratchpad(task_id)

        linked_task_ids = await self._ctx.task_service.get_task_links(task_id)
        linked_tasks: list[dict[str, Any]] = []
        for linked_task_id in linked_task_ids:
            linked = await self._ctx.task_service.get_task(linked_task_id)
            if linked is None:
                continue
            linked_tasks.append(
                {
                    "task_id": linked.id,
                    "title": linked.title,
                    "status": linked.status.value,
                    "description": linked.description,
                }
            )
        linked_tasks.sort(key=lambda item: item["task_id"])

        workspace_id: str | None = None
        workspace_branch: str | None = None
        workspace_path: str | None = None
        repos: list[dict[str, Any]] = []

        workspaces = await self._ctx.workspace_service.list_workspaces(task_id=task_id)
        if workspaces:
            workspace = workspaces[0]
            workspace_id = workspace.id
            workspace_branch = workspace.branch_name
            workspace_path = workspace.path
            try:
                workspace_repos = await self._ctx.workspace_service.get_workspace_repos(
                    workspace.id
                )
                repos = [
                    {
                        "repo_id": repo["repo_id"],
                        "name": repo["repo_name"],
                        "path": repo["repo_path"],
                        "worktree_path": repo.get("worktree_path"),
                        "target_branch": repo.get("target_branch"),
                        "has_changes": repo.get("has_changes"),
                    }
                    for repo in workspace_repos
                ]
            except (AttributeError, KeyError, LookupError, OSError, RuntimeError) as exc:
                logger.warning("API: workspace repos unavailable: %s", exc)

        if not repos:
            try:
                project_repos = await self._ctx.project_service.get_project_repos(task.project_id)
                repos = [
                    {
                        "repo_id": repo.id,
                        "name": repo.name,
                        "path": repo.path,
                        "worktree_path": None,
                        "target_branch": repo.default_branch,
                        "has_changes": None,
                    }
                    for repo in project_repos
                ]
            except (AttributeError, KeyError, LookupError, OSError, RuntimeError) as exc:
                logger.warning("API: project repos unavailable: %s", exc)

        return {
            "task_id": task.id,
            "project_id": task.project_id,
            "title": task.title,
            "description": task.description,
            "status": task.status.value,
            "acceptance_criteria": task.acceptance_criteria,
            "scratchpad": scratchpad,
            "workspace_id": workspace_id,
            "workspace_branch": workspace_branch,
            "workspace_path": workspace_path,
            "repos": repos,
            "repo_count": len(repos),
            "linked_tasks": linked_tasks,
        }

    @expose("tasks", "logs", description="Return execution logs for a task.")
    async def get_task_logs(self, task_id: str, *, limit: int = 5) -> list[dict[str, Any]]:
        """Return execution logs for a task."""
        limit = max(1, min(limit, 20))
        executions = await self._ctx.execution_service.list_executions_for_task(
            task_id, limit=limit
        )
        logs: list[dict[str, Any]] = []
        total_runs = len(executions)
        with contextlib.suppress(AttributeError, KeyError, RuntimeError):
            total_runs = max(
                total_runs,
                await self._ctx.execution_service.count_executions_for_task(task_id),
            )

        run_start = max(1, total_runs - len(executions) + 1)
        for run_number, execution in enumerate(reversed(executions), start=run_start):
            try:
                log_entries = await self._ctx.execution_service.get_execution_log_entries(
                    execution.id
                )
                content = "\n".join(entry.logs for entry in log_entries if entry.logs).strip()
                if not content:
                    continue
                logs.append(
                    {
                        "run": run_number,
                        "content": content,
                        "created_at": execution.created_at.isoformat(),
                    }
                )
            except (AttributeError, KeyError, RuntimeError):
                pass

        return logs

    @expose("tasks", "search", description="Search tasks by text query.")
    async def search_tasks(self, query: str) -> Sequence[Task]:
        """Search tasks by text query."""
        if not query.strip():
            return []
        return await self._ctx.task_service.search(query)

    # ── Reviews ────────────────────────────────────────────────────────

    @expose(
        "review",
        "request",
        profile="pair_worker",
        mutating=True,
        description="Mark task ready for review.",
    )
    async def request_review(self, task_id: str, summary: str = "") -> Task | None:
        """Move task to REVIEW status."""
        return await self._ctx.task_service.set_status(
            task_id, TaskStatus.REVIEW, reason="Review requested"
        )

    @expose(
        "review",
        "approve",
        profile="operator",
        mutating=True,
        description="Approve a task review.",
    )
    async def approve_task(self, task_id: str) -> Task | None:
        """Approve a task review."""
        return await self._ctx.task_service.sync_status_from_review_pass(task_id)

    @expose(
        "review",
        "reject",
        profile="operator",
        mutating=True,
        description="Reject a task review with feedback.",
    )
    async def reject_task(
        self,
        task_id: str,
        feedback: str = "",
        action: str = "reopen",
    ) -> Task | None:
        """Reject a task review with feedback."""
        task = await self._ctx.task_service.get_task(task_id)
        if task is None:
            return None
        return await self._ctx.merge_service.apply_rejection_feedback(task, feedback, action)

    async def merge_task(self, task_id: str) -> tuple[bool, str]:
        """Merge a task's workspace into the base branch.

        Returns:
            Tuple of (success, message).
        """
        task = await self._ctx.task_service.get_task(task_id)
        if task is None:
            return False, f"Task {task_id} not found"

        if self._ctx.config.general.require_review_approval and task.status == TaskStatus.REVIEW:
            return False, "Task review must be approved before merge"

        success, message = await self._ctx.merge_service.merge_task(task)
        if not success:
            refreshed = await self._ctx.task_service.get_task(task_id)
            if refreshed is not None and refreshed.status is not TaskStatus.REVIEW:
                restored = await self._ctx.task_service.move(task_id, TaskStatus.REVIEW)
                if restored is not None:
                    message = f"{message} Task returned to REVIEW for retry."
        return success, message

    async def rebase_task(
        self, task_id: str, *, base_branch: str | None = None
    ) -> tuple[bool, str, list[str]]:
        """Rebase task worktree onto base branch.

        Returns:
            Tuple of (success, message, conflict_files).
        """
        task = await self._ctx.task_service.get_task(task_id)
        if task is None:
            return False, f"Task {task_id} not found", []

        if task.status != TaskStatus.REVIEW:
            return False, "Task is not in REVIEW", []

        resolved_branch = (
            base_branch or task.base_branch or self._ctx.config.general.default_base_branch
        )

        success, message, conflict_files = await self._ctx.workspace_service.rebase_onto_base(
            task.id, resolved_branch
        )

        if success:
            return True, f"Rebased: {task.title}", []

        if not conflict_files:
            return False, f"Rebase failed: {message}", []

        await self._ctx.workspace_service.abort_rebase(task.id)

        await self._ctx.task_service.update_fields(
            task.id,
            description=(task.description or "") + "\n\n---\n_Rebase conflict detected_",
        )
        await self._ctx.task_service.move(task.id, TaskStatus.IN_PROGRESS)

        if task.task_type == TaskType.AUTO:
            refreshed = await self._ctx.task_service.get_task(task.id)
            if refreshed is not None:
                await self._ctx.automation_service.spawn_for_task(refreshed)

        return (
            False,
            f"Rebase conflict: {len(conflict_files)} file(s). Task moved to IN_PROGRESS.",
            conflict_files,
        )

    # ── Private helpers ────────────────────────────────────────────────

    async def _handle_task_type_transition(
        self,
        *,
        task_id: str,
        current_type: TaskType,
        new_type: TaskType,
        fields: dict[str, object],
    ) -> None:
        """Handle side effects when task_type changes between PAIR and AUTO."""
        if current_type == new_type:
            return

        if current_type == TaskType.PAIR and new_type == TaskType.AUTO:
            if await self._ctx.session_service.session_exists(task_id):
                await self._ctx.session_service.kill_session(task_id)
            fields["terminal_backend"] = None
            return

        if current_type == TaskType.AUTO and new_type == TaskType.PAIR:
            if self._ctx.automation_service.is_running(task_id):
                await self._ctx.automation_service.stop_task(task_id)
