"""MCP tool implementations for Kagan."""

from __future__ import annotations

import asyncio
from pathlib import Path

from kagan.constants import KAGAN_GENERATED_PATTERNS
from kagan.core.models.enums import MergeReadiness, TaskStatus
from kagan.services.tasks import TaskService  # noqa: TC001


class KaganMCPServer:
    """Handler for MCP tools backed by TaskService."""

    def __init__(self, state_manager: TaskService) -> None:
        self._state = state_manager

    async def get_context(self, task_id: str) -> dict:
        """Get task context for AI tools."""
        task = await self._state.get_task(task_id)
        if task is None:
            raise ValueError(f"Task not found: {task_id}")
        scratchpad = await self._state.get_scratchpad(task_id)
        return {
            "task_id": task.id,
            "title": task.title,
            "description": task.description,
            "acceptance_criteria": task.acceptance_criteria,
            "scratchpad": scratchpad,
        }

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
        has_uncommitted = await self._check_uncommitted_changes()
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

    async def _check_uncommitted_changes(self) -> bool:
        """Check if there are uncommitted changes in the working directory.

        Excludes Kagan-generated files from the check since they are
        local development metadata, not project files.
        """
        process = await asyncio.create_subprocess_shell(
            "git status --porcelain",
            cwd=Path.cwd(),
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
