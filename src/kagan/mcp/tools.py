"""MCP tool implementations for Kagan."""

from __future__ import annotations

import asyncio
from pathlib import Path
from typing import TYPE_CHECKING

from kagan.constants import KAGAN_GENERATED_PATTERNS
from kagan.core.models.enums import TaskStatus
from kagan.services.tasks import TaskService  # noqa: TC001

if TYPE_CHECKING:
    from kagan.services.executions import ExecutionService
    from kagan.services.projects import ProjectService
    from kagan.services.workspaces import WorkspaceService


class KaganMCPServer:
    """Handler for MCP tools backed by TaskService."""

    def __init__(
        self,
        state_manager: TaskService,
        *,
        workspace_service: WorkspaceService | None = None,
        project_service: ProjectService | None = None,
        execution_service: ExecutionService | None = None,
    ) -> None:
        self._state = state_manager
        self._workspaces = workspace_service
        self._projects = project_service
        self._executions = execution_service

    async def get_context(self, task_id: str) -> dict:
        """Get task context for AI tools."""
        task = await self._state.get_task(task_id)
        if task is None:
            raise ValueError(f"Task not found: {task_id}")
        scratchpad = await self._state.get_scratchpad(task_id)
        context: dict = {
            "task_id": task.id,
            "title": task.title,
            "description": task.description,
            "acceptance_criteria": task.acceptance_criteria,
            "scratchpad": scratchpad,
        }
        if self._workspaces:
            workspaces = await self._workspaces.list_workspaces(task_id=task_id)
            if workspaces:
                workspace = workspaces[0]
                repos = await self._workspaces.get_workspace_repos(workspace.id)
                try:
                    working_dir = await self._workspaces.get_agent_working_dir(workspace.id)
                except ValueError:
                    working_dir = None
                context.update(
                    {
                        "workspace_id": workspace.id,
                        "workspace_branch": workspace.branch_name,
                        "workspace_path": workspace.path,
                        "working_dir": str(working_dir) if working_dir else None,
                        "repos": [
                            {
                                "repo_id": repo["repo_id"],
                                "name": repo["repo_name"],
                                "path": repo["repo_path"],
                                "worktree_path": repo["worktree_path"],
                                "target_branch": repo["target_branch"],
                                "has_changes": repo["has_changes"],
                                "diff_stats": repo["diff_stats"],
                            }
                            for repo in repos
                        ],
                        "repo_count": len(repos),
                    }
                )
                return context

        if self._projects and getattr(task, "project_id", None):
            repos = await self._projects.get_project_repos(task.project_id)
            context.update(
                {
                    "repos": [
                        {
                            "repo_id": repo.id,
                            "name": repo.name,
                            "path": repo.path,
                            "target_branch": repo.default_branch,
                        }
                        for repo in repos
                    ],
                    "repo_count": len(repos),
                }
            )

        # Resolve @mention linked tasks
        linked_ids = await self._state.get_task_links(task_id)
        if linked_ids:
            linked_tasks = []
            for lid in linked_ids:
                linked = await self._state.get_task(lid)
                if linked:
                    linked_tasks.append(
                        {
                            "task_id": linked.id,
                            "title": linked.title,
                            "status": linked.status.value,
                            "description": linked.description,
                        }
                    )
            context["linked_tasks"] = linked_tasks

        return context

    async def update_scratchpad(self, task_id: str, content: str) -> bool:
        """Append to task scratchpad."""
        existing = await self._state.get_scratchpad(task_id)
        updated = f"{existing}\n{content}".strip() if existing else content
        await self._state.update_scratchpad(task_id, updated)
        return True

    async def request_review(self, task_id: str, summary: str) -> dict:
        """Mark task ready for review.

        For PAIR mode tasks, this moves the task to REVIEW status.
        AUTO mode tasks use agent-based review via the automation service instead.
        """
        task = await self._state.get_task(task_id)
        if task is None:
            raise ValueError(f"Task not found: {task_id}")

        has_uncommitted = await self._check_uncommitted_changes(task_id)
        if has_uncommitted:
            return {
                "status": "error",
                "message": "Cannot request review with uncommitted changes. "
                "Please commit your work first.",
            }

        await self._state.update_fields(task_id, status=TaskStatus.REVIEW)
        scratchpad = await self._state.get_scratchpad(task_id)
        note = f"\n\n--- REVIEW REQUEST ---\n{summary}"
        await self._state.update_scratchpad(task_id, scratchpad + note)
        return {"status": "review", "message": "Ready for merge"}

    async def _check_uncommitted_changes(self, task_id: str | None = None) -> bool:
        """Check if there are uncommitted changes in the working directory."""
        if self._workspaces and task_id:
            workspaces = await self._workspaces.list_workspaces(task_id=task_id)
            if workspaces:
                repos = await self._workspaces.get_workspace_repos(workspaces[0].id)
                paths = [Path(repo["worktree_path"]) for repo in repos if repo.get("worktree_path")]
                for path in paths:
                    if await self._has_uncommitted_changes(path):
                        return True
                return False
        return await self._has_uncommitted_changes(Path.cwd())

    async def _has_uncommitted_changes(self, path: Path) -> bool:
        if not path.exists():
            return False
        process = await asyncio.create_subprocess_shell(
            "git status --porcelain",
            cwd=path,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.DEVNULL,
        )
        stdout, _ = await process.communicate()

        if not stdout.strip():
            return False

        for line in stdout.decode().strip().split("\n"):
            if not line:
                continue

            filepath = line[3:].split(" -> ")[0]

            is_kagan_file = any(
                filepath.startswith(p.rstrip("/")) or filepath == p.rstrip("/")
                for p in KAGAN_GENERATED_PATTERNS
            )
            if not is_kagan_file:
                return True

        return False

    async def get_task(
        self,
        task_id: str,
        *,
        include_scratchpad: bool | None = None,
        include_logs: bool | None = None,
        include_review: bool | None = None,
        mode: str = "summary",
    ) -> dict:
        """Get task details with optional extended context."""
        del mode
        task = await self._state.get_task(task_id)
        if task is None:
            raise ValueError(f"Task not found: {task_id}")
        result: dict = {
            "task_id": task.id,
            "title": task.title,
            "status": task.status.value,
            "description": task.description,
            "acceptance_criteria": task.acceptance_criteria,
        }
        if include_scratchpad:
            result["scratchpad"] = await self._state.get_scratchpad(task_id)
        if include_logs and self._executions:
            logs = await self._get_agent_logs(task_id)
            result["logs"] = logs
        if include_review and self._executions:
            result["review_feedback"] = await self._get_review_feedback(task_id)
        return result

    async def get_parallel_tasks(self, exclude_task_id: str | None = None) -> list[dict]:
        """Get all IN_PROGRESS tasks for coordination awareness.

        Args:
            exclude_task_id: Optionally exclude a task (caller's own task).

        Returns:
            List of task summaries: task_id, title, description, scratchpad.
        """
        tasks = await self._state.get_by_status(TaskStatus.IN_PROGRESS)
        result = []
        for t in tasks:
            if exclude_task_id and t.id == exclude_task_id:
                continue
            scratchpad = await self._state.get_scratchpad(t.id)
            result.append(
                {
                    "task_id": t.id,
                    "title": t.title,
                    "description": t.description,
                    "scratchpad": scratchpad,
                }
            )
        return result

    async def _get_agent_logs(
        self, task_id: str, log_type: str = "implementation", limit: int = 1
    ) -> list[dict]:
        """Get agent execution logs for a task.

        Args:
            task_id: The task to get logs for.
            log_type: 'implementation' or 'review'.
            limit: Max runs to return (most recent).

        Returns:
            List of log entries with run, content, created_at.
        """
        task = await self._state.get_task(task_id)
        if task is None:
            raise ValueError(f"Task not found: {task_id}")

        if self._executions is None:
            return []

        execution = await self._executions.get_latest_execution_for_task(task_id)
        if execution is None:
            return []

        log_entry = await self._executions.get_logs(execution.id)
        if log_entry is None:
            return []

        run_count = await self._executions.count_executions_for_task(task_id)

        return [
            {
                "run": run_count,
                "content": log_entry.logs,
                "created_at": log_entry.inserted_at.isoformat(),
            }
        ]

    async def _get_review_feedback(self, task_id: str) -> str | None:
        if self._executions is None:
            return None
        execution = await self._executions.get_latest_execution_for_task(task_id)
        if execution is None:
            return None
        return _format_review_feedback(execution.metadata_.get("review_result"))


def _format_review_feedback(review_result: object) -> str | None:
    if not isinstance(review_result, dict):
        return None
    summary = str(review_result.get("summary") or "").strip()
    status = review_result.get("status")
    approved = review_result.get("approved")
    if status is None and isinstance(approved, bool):
        status = "approved" if approved else "rejected"
    status_label = str(status).strip().lower() if status is not None else ""
    if summary:
        return f"{status_label}: {summary}" if status_label else summary
    if status_label:
        return f"Review {status_label}."
    return None
