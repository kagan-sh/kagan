"""Ticket lifecycle operations - decoupled from UI."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

from kagan.database.models import MergeReadiness, TicketStatus

if TYPE_CHECKING:
    from kagan.agents.scheduler import Scheduler
    from kagan.agents.worktree import WorktreeManager
    from kagan.config import KaganConfig
    from kagan.database.manager import StateManager
    from kagan.database.models import Ticket
    from kagan.sessions.manager import SessionManager

log = logging.getLogger(__name__)


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


class TicketLifecycle:
    """Manages ticket lifecycle operations without UI coupling."""

    def __init__(
        self,
        state: StateManager,
        worktrees: WorktreeManager,
        sessions: SessionManager,
        scheduler: Scheduler,
        config: KaganConfig,
    ) -> None:
        self.state = state
        self.worktrees = worktrees
        self.sessions = sessions
        self.scheduler = scheduler
        self.config = config

    async def delete_ticket(self, ticket: Ticket) -> tuple[bool, str]:
        """Delete ticket with rollback-aware error handling.

        Returns:
            Tuple of (success, message) indicating result and reason.
        """
        steps_completed: list[str] = []
        try:
            # Step 1: Stop agent if running
            if self.scheduler.is_running(ticket.id):
                await self.scheduler.stop_ticket(ticket.id)
            steps_completed.append("agent_stopped")

            # Step 2: Kill session
            await self.sessions.kill_session(ticket.id)
            steps_completed.append("session_killed")

            # Step 3: Delete worktree
            if await self.worktrees.get_path(ticket.id):
                await self.worktrees.delete(ticket.id, delete_branch=True)
            steps_completed.append("worktree_deleted")

            # Step 4: Delete from database (point of no return)
            await self.state.delete_ticket(ticket.id)
            steps_completed.append("db_deleted")

            log.debug(f"Ticket {ticket.id} deleted successfully. Steps: {steps_completed}")
            return True, "Deleted successfully"
        except Exception as e:
            log.error(
                f"Delete failed for ticket {ticket.id} after steps: {steps_completed}. Error: {e}"
            )
            return False, f"Delete failed: {e}"

    async def merge_ticket(self, ticket: Ticket) -> tuple[bool, str]:
        """Merge ticket changes and clean up. Returns (success, message)."""
        base = self.config.general.default_base_branch
        config = self.config.general

        if config.require_review_approval and ticket.checks_passed is not True:
            message = "Review approval required before merge."
            await self.state.update_ticket(
                ticket.id,
                merge_failed=True,
                merge_error=message,
                merge_readiness=MergeReadiness.BLOCKED,
            )
            await self.state.append_ticket_event(ticket.id, "policy", message)
            return False, message

        async def _do_merge() -> tuple[bool, str]:
            await self.state.update_ticket(
                ticket.id,
                merge_failed=False,
                merge_error=None,
                merge_readiness=MergeReadiness.RISK,
            )

            success, message = await self.worktrees.merge_to_main(  # type: ignore[misc]
                ticket.id, base_branch=base, allow_conflicts=True
            )
            if success:
                await self.worktrees.delete(ticket.id, delete_branch=True)
                await self.sessions.kill_session(ticket.id)
                await self.state.update_ticket(
                    ticket.id,
                    status=TicketStatus.DONE,
                    merge_failed=False,
                    merge_error=None,
                    merge_readiness=MergeReadiness.READY,
                )
                await self.state.append_ticket_event(ticket.id, "merge", f"Merged to {base}")
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

                    await self.state.update_ticket(
                        ticket.id,
                        merge_failed=True,
                        merge_error=final_message[:500],
                        merge_readiness=MergeReadiness.BLOCKED,
                    )
                    await self.state.append_ticket_event(
                        ticket.id, "merge", f"Merge conflict: {error_msg}"
                    )
                else:
                    # Non-conflict failures: keep in REVIEW with generic error
                    await self.state.update_ticket(
                        ticket.id,
                        merge_failed=True,
                        merge_error=message[:500] if message else "Unknown error",
                        merge_readiness=MergeReadiness.BLOCKED,
                    )
                    await self.state.append_ticket_event(
                        ticket.id, "merge", f"Merge failed: {message}"
                    )

            return success, message

        if config.serialize_merges:
            async with self.scheduler.merge_lock:
                return await _do_merge()
        return await _do_merge()

    async def close_exploratory(self, ticket: Ticket) -> tuple[bool, str]:
        """Close a DONE ticket by deleting it (used for no-change exploratory tasks)."""
        if await self.worktrees.get_path(ticket.id):
            await self.worktrees.delete(ticket.id, delete_branch=True)
        await self.sessions.kill_session(ticket.id)

        # Stop agent if running
        if self.scheduler.is_running(ticket.id):
            await self.scheduler.stop_ticket(ticket.id)

        # Delete ticket (exploratory tasks are removed, not kept as DONE)
        await self.state.delete_ticket(ticket.id)
        return True, "Closed as exploratory"

    async def apply_rejection_feedback(
        self,
        ticket: Ticket,
        feedback: str | None,
        action: str = "shelve",  # "retry" | "stage" | "shelve"
    ) -> Ticket:
        """Apply rejection feedback with state transition per Active Iteration Model.

        State Transitions:
            - retry: REVIEW → IN_PROGRESS (agent spawned, iterations reset)
            - stage: REVIEW → IN_PROGRESS (agent paused, iterations reset)
            - shelve: REVIEW → BACKLOG (iterations preserved)

        Returns:
            Updated ticket from database.
        """
        # Determine target status based on action
        target_status = TicketStatus.BACKLOG if action == "shelve" else TicketStatus.IN_PROGRESS

        # Append feedback to description if provided
        if feedback:
            from datetime import datetime

            timestamp = datetime.now().strftime("%Y-%m-%d %H:%M")
            new_description = ticket.description or ""
            new_description += f"\n\n---\n**Review Feedback ({timestamp}):**\n{feedback}"

            await self.state.update_ticket(
                ticket.id,
                description=new_description,
                status=target_status,
                merge_failed=False,
                merge_error=None,
                merge_readiness=MergeReadiness.RISK,
            )
            await self.state.append_ticket_event(
                ticket.id, "review", f"Rejected with feedback: {feedback[:200]}"
            )
        else:
            await self.state.update_ticket(
                ticket.id,
                status=target_status,
                merge_failed=False,
                merge_error=None,
                merge_readiness=MergeReadiness.RISK,
            )
            await self.state.append_ticket_event(ticket.id, "review", "Rejected")

        # Reset iterations for retry/stage actions (not shelve)
        if action in ("retry", "stage"):
            self.scheduler.reset_iterations(ticket.id)

        # Return refreshed ticket
        refreshed_ticket = await self.state.get_ticket(ticket.id)
        assert refreshed_ticket is not None
        return refreshed_ticket

    async def has_no_changes(self, ticket: Ticket) -> bool:
        """Return True if the ticket has no commits and no diff stats."""
        base = self.config.general.default_base_branch
        commits = await self.worktrees.get_commit_log(ticket.id, base_branch=base)
        diff_stats = await self.worktrees.get_diff_stats(ticket.id, base_branch=base)
        return not commits and not diff_stats.strip()

    async def get_review_ticket(self, ticket: Ticket) -> Ticket | None:
        """Get REVIEW ticket for same worktree branch.

        Returns:
            Ticket in REVIEW status sharing the same branch, or None.
        """
        branch_name = await self.worktrees.get_branch_name(ticket.id)
        if not branch_name:
            return None

        # Get all tickets in REVIEW
        all_tickets = await self.state.get_all_tickets()
        review_tickets = [t for t in all_tickets if t.status == TicketStatus.REVIEW]

        # Find one with matching branch
        for review_ticket in review_tickets:
            review_branch = await self.worktrees.get_branch_name(review_ticket.id)
            if review_branch == branch_name:
                return review_ticket

        return None
