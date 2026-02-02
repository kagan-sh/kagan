"""Actions for Kanban screen ticket operations."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

from kagan.database.models import TicketStatus, TicketType

if TYPE_CHECKING:
    from kagan.app import KaganApp
    from kagan.database.models import Ticket

log = logging.getLogger(__name__)


async def delete_ticket(app: KaganApp, ticket: Ticket) -> tuple[bool, str]:
    """Delete ticket with rollback-aware error handling.

    Tracks completed steps for debugging if any step fails mid-deletion.

    Returns:
        Tuple of (success, message) indicating result and reason.
    """
    steps_completed: list[str] = []
    try:
        # Step 1: Stop agent if running
        scheduler = app.scheduler
        if scheduler.is_running(ticket.id):
            agent = scheduler.get_running_agent(ticket.id)
            if agent:
                await agent.stop()
        steps_completed.append("agent_stopped")

        # Step 2: Kill session
        await app.session_manager.kill_session(ticket.id)
        steps_completed.append("session_killed")

        # Step 3: Delete worktree
        worktree = app.worktree_manager
        if await worktree.get_path(ticket.id):
            await worktree.delete(ticket.id, delete_branch=True)
        steps_completed.append("worktree_deleted")

        # Step 4: Delete from database (point of no return)
        await app.state_manager.delete_ticket(ticket.id)
        steps_completed.append("db_deleted")

        log.debug(f"Ticket {ticket.id} deleted successfully. Steps: {steps_completed}")
        return True, "Deleted successfully"
    except Exception as e:
        log.error(
            f"Delete failed for ticket {ticket.id} after steps: {steps_completed}. Error: {e}"
        )
        return False, f"Delete failed: {e}"


async def merge_ticket(app: KaganApp, ticket: Ticket) -> tuple[bool, str]:
    """Merge ticket changes and clean up. Returns (success, message)."""
    worktree = app.worktree_manager
    base = app.config.general.default_base_branch

    success, message = await worktree.merge_to_main(ticket.id, base_branch=base)
    if success:
        await worktree.delete(ticket.id, delete_branch=True)
        await app.session_manager.kill_session(ticket.id)
        await app.state_manager.move_ticket(ticket.id, TicketStatus.DONE)

    return success, message


async def apply_rejection_feedback(
    app: KaganApp,
    ticket: Ticket,
    feedback: str | None,
    action: str = "retry",  # "retry" | "stage" | "shelve"
) -> None:
    """Apply rejection feedback with state transition per Active Iteration Model.

    State Transitions:
        - retry: REVIEW → IN_PROGRESS (agent spawned, iterations reset)
        - stage: REVIEW → IN_PROGRESS (agent paused, iterations reset)
        - shelve: REVIEW → BACKLOG (iterations preserved)

    Args:
        app: The KaganApp instance
        ticket: The ticket being rejected
        feedback: Optional feedback text to append to description
        action: The action to take ("retry", "stage", or "shelve")
    """
    # Determine target status based on action
    target_status = TicketStatus.BACKLOG if action == "shelve" else TicketStatus.IN_PROGRESS

    # Append feedback to description if provided
    if feedback:
        from datetime import datetime

        timestamp = datetime.now().strftime("%Y-%m-%d %H:%M")
        new_description = ticket.description or ""
        new_description += f"\n\n---\n**Review Feedback ({timestamp}):**\n{feedback}"

        await app.state_manager.update_ticket(
            ticket.id,
            description=new_description,
            status=target_status,
        )
    else:
        await app.state_manager.move_ticket(ticket.id, target_status)

    # Reset iterations for retry/stage actions (not shelve)
    if action in ("retry", "stage"):
        app.scheduler.reset_iterations(ticket.id)

    # Auto-restart agent for retry action on AUTO tickets
    if action == "retry" and ticket.ticket_type == TicketType.AUTO:
        # Refresh ticket from DB to get updated state
        refreshed_ticket = await app.state_manager.get_ticket(ticket.id)
        if refreshed_ticket:
            await app.scheduler.spawn_for_ticket(refreshed_ticket)


def get_review_ticket(screen, card) -> Ticket | None:
    """Get ticket from card if it's in REVIEW status."""
    if not card or not card.ticket:
        return None
    if card.ticket.status != TicketStatus.REVIEW:
        screen.notify("Ticket is not in REVIEW", severity="warning")
        return None
    return card.ticket
