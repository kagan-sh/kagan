"""MCP tool implementations for Kagan."""

from __future__ import annotations

import asyncio
from pathlib import Path
from typing import TYPE_CHECKING

from kagan.constants import KAGAN_GENERATED_PATTERNS
from kagan.core.models.enums import MergeReadiness, TaskStatus
from kagan.services.tasks import TaskService  # noqa: TC001

if TYPE_CHECKING:
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
    ) -> None:
        self._state = state_manager
        self._workspaces = workspace_service
        self._projects = project_service

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

        # Check for uncommitted changes before allowing review
        has_uncommitted = await self._check_uncommitted_changes(task_id)
        if has_uncommitted:
            return {
                "status": "error",
                "message": "Cannot request review with uncommitted changes. "
                "Please commit your work first.",
            }

        await self._state.update_fields(
            task_id,
            review_summary=summary,
            checks_passed=None,
            status=TaskStatus.REVIEW,
            merge_failed=False,
            merge_error=None,
            merge_readiness=MergeReadiness.RISK,
        )
        await self._state.append_event(task_id, "review", "Review requested")
        return {"status": "review", "message": "Ready for merge"}

    async def _check_uncommitted_changes(self, task_id: str | None = None) -> bool:
        """Check if there are uncommitted changes in the working directory.

        Excludes Kagan-generated files from the check since they are
        local development metadata, not project files.
        """
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

        # Filter out Kagan-generated files
        for line in stdout.decode().strip().split("\n"):
            if not line:
                continue
            # git status --porcelain format: "XY filename" or "XY  filename -> newname"
            # The filename starts at position 3
            filepath = line[3:].split(" -> ")[0]
            # Check if this file matches any Kagan pattern
            is_kagan_file = any(
                filepath.startswith(p.rstrip("/")) or filepath == p.rstrip("/")
                for p in KAGAN_GENERATED_PATTERNS
            )
            if not is_kagan_file:
                return True  # Found a non-Kagan uncommitted change

        return False  # Only Kagan files are uncommitted

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

    async def get_agent_logs(
        self, task_id: str, log_type: str = "implementation", limit: int = 1
    ) -> list[dict]:
        """Get agent execution logs for a task.

        Args:
            task_id: The task to get logs for.
            log_type: 'implementation' or 'review'.
            limit: Max iterations to return (most recent).

        Returns:
            List of log entries with iteration, content, created_at.
        """
        task = await self._state.get_task(task_id)
        if task is None:
            raise ValueError(f"Task not found: {task_id}")

        logs = await self._state.get_agent_logs(task_id, log_type)
        # Get most recent N logs, then reverse for ascending order
        logs = sorted(logs, key=lambda x: x.sequence, reverse=True)[:limit]
        logs = list(reversed(logs))  # O(n) instead of O(n log n)
        return [
            {
                "iteration": log.sequence,
                "content": log.content,
                "created_at": log.created_at.isoformat(),
            }
            for log in logs
        ]
