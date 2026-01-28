"""KanbanColumn widget for displaying a status column."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Container, ScrollableContainer, Vertical
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Label

from kagan.constants import STATUS_LABELS
from kagan.database.models import Ticket, TicketStatus
from kagan.ui.widgets.card import TicketCard

if TYPE_CHECKING:
    from textual.app import ComposeResult


class _NSLabel(Label):
    ALLOW_SELECT = False
    can_focus = False


class _NSVertical(Vertical):
    ALLOW_SELECT = False
    can_focus = False


class _NSScrollable(ScrollableContainer):
    ALLOW_SELECT = False
    can_focus = False


class _NSContainer(Container):
    ALLOW_SELECT = False
    can_focus = False


class KanbanColumn(Widget):
    ALLOW_SELECT = False
    can_focus = False

    status: reactive[TicketStatus] = reactive(TicketStatus.BACKLOG)

    def __init__(self, status: TicketStatus, tickets: list[Ticket] | None = None, **kwargs) -> None:
        super().__init__(id=f"column-{status.value.lower()}", **kwargs)
        self.status = status
        self._tickets: list[Ticket] = tickets or []

    def compose(self) -> ComposeResult:
        with _NSVertical():
            with _NSVertical(classes="column-header"):
                yield _NSLabel(
                    f"{STATUS_LABELS[self.status]} ({len(self._tickets)})",
                    id=f"header-{self.status.value.lower()}",
                    classes="column-header-text",
                )
            with _NSScrollable(classes="column-content", id=f"content-{self.status.value.lower()}"):
                if self._tickets:
                    for ticket in self._tickets:
                        yield TicketCard(ticket)
                else:
                    empty_id = f"empty-{self.status.value.lower()}"
                    with _NSContainer(classes="column-empty", id=empty_id):
                        yield _NSLabel("No tickets", classes="empty-message")

    def get_cards(self) -> list[TicketCard]:
        return list(self.query(TicketCard))

    def get_focused_card_index(self) -> int | None:
        for i, card in enumerate(self.get_cards()):
            if card.has_focus:
                return i
        return None

    def focus_card(self, index: int) -> bool:
        cards = self.get_cards()
        if 0 <= index < len(cards):
            cards[index].focus()
            return True
        return False

    def focus_first_card(self) -> bool:
        return self.focus_card(0)

    def update_tickets(self, tickets: list[Ticket]) -> None:
        """Update tickets with minimal DOM changes - no full recompose.

        Only adds new cards and removes cards for tickets that moved out.
        Existing cards are kept as-is since ticket metadata rarely changes
        (status changes cause tickets to move between columns).
        """
        new_tickets = [t for t in tickets if t.status == self.status]
        self._tickets = new_tickets

        # Update header count
        try:
            header = self.query_one(f"#header-{self.status.value.lower()}", _NSLabel)
            header.update(f"{STATUS_LABELS[self.status]} ({len(new_tickets)})")
        except Exception:
            pass

        # Get current cards and build lookup
        current_cards = {card.ticket.id: card for card in self.get_cards() if card.ticket}
        new_ticket_ids = {t.id for t in new_tickets}
        current_ids = set(current_cards.keys())

        try:
            content = self.query_one(f"#content-{self.status.value.lower()}", _NSScrollable)
        except Exception:
            return

        # Remove cards for tickets no longer in this column
        for ticket_id in current_ids - new_ticket_ids:
            card = current_cards[ticket_id]
            card.remove()

        # Add new cards only (tickets that weren't here before)
        for ticket in new_tickets:
            if ticket.id not in current_ids:
                content.mount(TicketCard(ticket))

        # Handle empty state container
        empty_id = f"empty-{self.status.value.lower()}"
        has_empty = False
        try:
            empty_container = self.query_one(f"#{empty_id}", _NSContainer)
            has_empty = True
            if new_tickets:
                # Have tickets now, remove empty state
                empty_container.remove()
        except Exception:
            pass

        # If no tickets and no empty container, add empty state
        if not new_tickets and not has_empty:
            empty = _NSContainer(classes="column-empty", id=empty_id)
            content.mount(empty)
            empty.mount(_NSLabel("No tickets", classes="empty-message"))

    def update_active_states(self, active_ids: set[str]) -> None:
        """Update active agent state for all cards in this column."""
        for card in self.query(TicketCard):
            if card.ticket is not None:
                card.is_agent_active = card.ticket.id in active_ids

    def update_iterations(self, iterations: dict[str, str]) -> None:
        """Update iteration display on cards."""
        for card in self.query(TicketCard):
            if card.ticket:
                card.iteration_info = iterations.get(card.ticket.id, "")
