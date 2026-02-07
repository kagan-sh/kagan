"""Merge service operations - decoupled from UI."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Protocol

from kagan.core.models.enums import MergeReadiness, TaskStatus

if TYPE_CHECKING:
    from kagan.adapters.git.worktrees import WorktreeManager
    from kagan.config import KaganConfig
    from kagan.core.models.entities import Task
    from kagan.services.automation import AutomationServiceImpl
    from kagan.services.sessions import SessionService
    from kagan.services.tasks import TaskService

log = logging.getLogger(__name__)


class MergeService(Protocol):
    """Service interface for merge operations."""

    async def delete_task(self, task: Task) -> tuple[bool, str]: ...

    async def merge_task(self, task: Task) -> tuple[bool, str]: ...

    async def close_exploratory(self, task: Task) -> tuple[bool, str]: ...

    async def apply_rejection_feedback(
        self,
        task: Task,
        feedback: str | None,
        action: str = "shelve",
    ) -> Task: ...

    async def has_no_changes(self, task: Task) -> bool: ...


def _parse_conflict_files(git_output: str) -> list[str]:
    """Extract conflicted file paths from git merge output.

    Looks for pattern: "CONFLICT (content): Merge conflict in <file>"

    Args:
        git_output: Raw git merge stderr/stdout

    Returns:
        List of conflicted file paths
    """
    import re

    pattern = r"CONFLICT \([^)]+\): Merge conflict in (.+)"
    matches = re.findall(pattern, git_output)
    return [match.strip() for match in matches]


def _is_merge_conflict(message: str) -> bool:
    """Check if merge failure message indicates conflicts.

    Args:
        message: Error message from merge operation

    Returns:
        True if message contains conflict indicators
    """
    conflict_indicators = [
        "CONFLICT",
        "Merge conflict",
        "conflict in",
        "fix conflicts",
    ]
    return any(indicator.lower() in message.lower() for indicator in conflict_indicators)


class MergeServiceImpl:
    """Manages merge lifecycle operations without UI coupling."""

    def __init__(
        self,
        task_service: TaskService,
        worktrees: WorktreeManager,
        sessions: SessionService,
        automation: AutomationServiceImpl,
        config: KaganConfig,
    ) -> None:
        self.tasks = task_service
        self.worktrees = worktrees
        self.sessions = sessions
        self.automation = automation
        self.config = config

    async def delete_task(self, task: Task) -> tuple[bool, str]:
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

    async def merge_task(self, task: Task) -> tuple[bool, str]:
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

            success, message = await self.worktrees.merge_to_main(  # type: ignore[misc]
                task.id, base_branch=base, allow_conflicts=True
            )
            if success:
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
            else:
                # On merge conflict, stay in REVIEW with structured error
                if _is_merge_conflict(message):
                    conflict_files = _parse_conflict_files(message)
                    if conflict_files:
                        error_msg = f"Merge conflicts in: {', '.join(conflict_files)}"
                        hint = " Resolve conflicts and retry merge from REVIEW."
                    else:
                        error_msg = "Merge conflicts detected"
                        hint = " Check git status in worktree and retry."

                    final_message = error_msg + hint

                    await self.tasks.update_fields(
                        task.id,
                        merge_failed=True,
                        merge_error=final_message[:500],
                        merge_readiness=MergeReadiness.BLOCKED,
                    )
                    await self.tasks.append_event(task.id, "merge", f"Merge conflict: {error_msg}")
                else:
                    # Non-conflict failures: keep in REVIEW with generic error
                    await self.tasks.update_fields(
                        task.id,
                        merge_failed=True,
                        merge_error=message[:500] if message else "Unknown error",
                        merge_readiness=MergeReadiness.BLOCKED,
                    )
                    await self.tasks.append_event(task.id, "merge", f"Merge failed: {message}")

            return success, message

        if config.serialize_merges:
            async with self.automation.merge_lock:
                return await _do_merge()
        return await _do_merge()

    async def close_exploratory(self, task: Task) -> tuple[bool, str]:
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
        task: Task,
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

    async def has_no_changes(self, task: Task) -> bool:
        """Return True if the task has no commits and no diff stats."""
        base = self.config.general.default_base_branch
        commits = await self.worktrees.get_commit_log(task.id, base_branch=base)
        diff_stats = await self.worktrees.get_diff_stats(task.id, base_branch=base)
        return not commits and not diff_stats.strip()
