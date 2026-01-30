"""Actions for Kanban screen ticket operations."""

from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.database.models import TicketStatus, TicketUpdate

if TYPE_CHECKING:
    from kagan.app import KaganApp
    from kagan.database.models import Ticket


async def delete_ticket(app: KaganApp, ticket: Ticket) -> None:
    """Delete a ticket and clean up associated resources."""
    scheduler = app.scheduler
    if scheduler.is_running(ticket.id):
        agent = scheduler.get_running_agent(ticket.id)
        if agent:
            await agent.stop()

    await app.session_manager.kill_session(ticket.id)

    worktree = app.worktree_manager
    if await worktree.get_path(ticket.id):
        await worktree.delete(ticket.id, delete_branch=True)

    await app.state_manager.delete_ticket(ticket.id)


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


async def apply_rejection_feedback(app: KaganApp, ticket: Ticket, feedback: str | None) -> None:
    """Append rejection feedback to ticket and move to IN_PROGRESS."""
    if feedback:
        from datetime import datetime

        timestamp = datetime.now().strftime("%Y-%m-%d %H:%M")
        new_description = ticket.description or ""
        new_description += f"\n\n---\n**Review Feedback ({timestamp}):**\n{feedback}"

        await app.state_manager.update_ticket(
            ticket.id,
            TicketUpdate(description=new_description, status=TicketStatus.IN_PROGRESS),
        )
    else:
        await app.state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)


def get_review_ticket(screen, card) -> Ticket | None:
    """Get ticket from card if it's in REVIEW status."""
    if not card or not card.ticket:
        return None
    if card.ticket.status != TicketStatus.REVIEW:
        screen.notify("Ticket is not in REVIEW", severity="warning")
        return None
    return card.ticket
