"""KanbanColumn widget for displaying a status column."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import ScrollableContainer, Vertical
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


class KanbanColumn(Widget):
    ALLOW_SELECT = False
    can_focus = False

    status: reactive[TicketStatus] = reactive(TicketStatus.BACKLOG)
    tickets: reactive[list[Ticket]] = reactive(list, recompose=True)

    def __init__(self, status: TicketStatus, tickets: list[Ticket] | None = None, **kwargs) -> None:
        super().__init__(id=f"column-{status.value.lower()}", **kwargs)
        self.status = status
        self.tickets = tickets or []

    def compose(self) -> ComposeResult:
        with _NSVertical():
            with _NSVertical(classes="column-header"):
                yield _NSLabel(
                    f"{STATUS_LABELS[self.status]} ({len(self.tickets)})",
                    classes="column-header-text",
                )
            with _NSScrollable(classes="column-content"):
                if self.tickets:
                    for ticket in self.tickets:
                        yield TicketCard(ticket)
                else:
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
        self.tickets = [t for t in tickets if t.status == self.status]

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
