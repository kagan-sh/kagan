"""Validation logic for Kanban screen actions."""

from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.database.models import TicketStatus, TicketType
from kagan.ui.utils import coerce_enum

if TYPE_CHECKING:
    from kagan.agents.scheduler import Scheduler
    from kagan.database.models import Ticket


class ActionValidator:
    """Validates whether actions can be performed on tickets.

    Encapsulates all validation logic for ticket operations based on
    ticket status, type, and current agent state.
    """

    # Actions that require a ticket to be selected
    TICKET_REQUIRED_ACTIONS = frozenset(
        {
            "edit_ticket",
            "delete_ticket",
            "delete_ticket_direct",
            "view_details",
            "open_session",
            "move_forward",
            "move_backward",
            "duplicate_ticket",
            "merge",
            "merge_direct",
            "view_diff",
            "open_review",
            "watch_agent",
            "start_agent",
            "stop_agent",
        }
    )

    def __init__(self, scheduler: Scheduler) -> None:
        """Initialize validator with scheduler for agent state checks.

        Args:
            scheduler: The Scheduler instance for checking running agents.
        """
        self._scheduler = scheduler

    def validate(self, action: str, ticket: Ticket | None) -> tuple[bool, str | None]:
        """Validate if an action can be performed.

        Args:
            action: The action name to validate.
            ticket: The ticket to validate against (may be None).

        Returns:
            Tuple of (is_valid, reason). If is_valid is False, reason explains why.
            If is_valid is True, reason is None.
        """
        # No ticket - check ticket-requiring actions
        if not ticket:
            if action in self.TICKET_REQUIRED_ACTIONS:
                return (False, "No ticket selected")
            return (True, None)

        # Coerce enums once at the top
        status = coerce_enum(ticket.status, TicketStatus)
        ticket_type = coerce_enum(ticket.ticket_type, TicketType)

        # Validate based on action type
        if action == "edit_ticket":
            return self.can_edit(ticket, status)

        if action in ("move_forward", "move_backward"):
            return self.can_move(ticket, status, ticket_type)

        if action in ("merge", "merge_direct", "view_diff", "open_review"):
            return self.can_review(status)

        if action in ("watch_agent", "start_agent"):
            return self.can_start_agent(ticket_type)

        if action == "stop_agent":
            return self.can_stop_agent(ticket, ticket_type)

        return (True, None)

    def can_move(
        self, ticket: Ticket, status: TicketStatus, ticket_type: TicketType
    ) -> tuple[bool, str | None]:
        """Check if ticket can be moved to a different status.

        Args:
            ticket: The ticket to check.
            status: Current ticket status.
            ticket_type: Type of the ticket (AUTO/PAIR).

        Returns:
            Tuple of (can_move, reason).
        """
        # Done tickets cannot be moved
        if status == TicketStatus.DONE:
            return (False, "Done tickets cannot be moved. Use [y] to duplicate.")

        # AUTO in IN_PROGRESS - agent controls movement
        if status == TicketStatus.IN_PROGRESS and ticket_type == TicketType.AUTO:
            return (False, "Agent controls AUTO ticket movement")

        return (True, None)

    def can_delete(self, ticket: Ticket) -> tuple[bool, str | None]:
        """Check if ticket can be deleted.

        Args:
            ticket: The ticket to check.

        Returns:
            Tuple of (can_delete, reason).
        """
        # Currently all tickets can be deleted
        return (True, None)

    def can_edit(
        self, ticket: Ticket, status: TicketStatus | None = None
    ) -> tuple[bool, str | None]:
        """Check if ticket can be edited.

        Args:
            ticket: The ticket to check.
            status: Optional pre-coerced status.

        Returns:
            Tuple of (can_edit, reason).
        """
        if status is None:
            status = coerce_enum(ticket.status, TicketStatus)

        if status == TicketStatus.DONE:
            return (False, "Done tickets cannot be edited. Use [y] to duplicate.")
        return (True, None)

    def can_start_agent(self, ticket_type: TicketType) -> tuple[bool, str | None]:
        """Check if an agent can be started for this ticket.

        Args:
            ticket_type: Type of the ticket.

        Returns:
            Tuple of (can_start, reason).
        """
        if ticket_type != TicketType.AUTO:
            return (False, "Only available for AUTO tickets")
        return (True, None)

    def can_stop_agent(
        self, ticket: Ticket, ticket_type: TicketType | None = None
    ) -> tuple[bool, str | None]:
        """Check if the running agent can be stopped.

        Args:
            ticket: The ticket to check.
            ticket_type: Optional pre-coerced ticket type.

        Returns:
            Tuple of (can_stop, reason).
        """
        if ticket_type is None:
            ticket_type = coerce_enum(ticket.ticket_type, TicketType)

        if ticket_type != TicketType.AUTO:
            return (False, "Only available for AUTO tickets")

        if not self._scheduler.is_running(ticket.id):
            return (False, "No agent running for this ticket")

        return (True, None)

    def can_review(self, status: TicketStatus) -> tuple[bool, str | None]:
        """Check if ticket is in reviewable state.

        Args:
            status: Current ticket status.

        Returns:
            Tuple of (can_review, reason).
        """
        if status != TicketStatus.REVIEW:
            return (False, f"Only available for REVIEW tickets (current: {status.value})")
        return (True, None)
