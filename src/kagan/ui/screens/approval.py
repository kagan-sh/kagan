"""Approval screen for reviewing proposed tickets from planner."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual import on
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, DataTable, Static

from kagan.constants import APPROVAL_TITLE_MAX_LENGTH
from kagan.database.models import TicketCreate, TicketType
from kagan.keybindings import APPROVAL_BINDINGS, to_textual_bindings
from kagan.ui.screens.ticket_editor import TicketEditorScreen

if TYPE_CHECKING:
    from textual.app import ComposeResult


class ApprovalScreen(ModalScreen[list[TicketCreate] | None]):
    """Review and approve proposed tickets.

    Returns:
        list[TicketCreate]: Approved tickets (possibly with modified types)
        None: User cancelled
    """

    BINDINGS = to_textual_bindings(APPROVAL_BINDINGS)

    def __init__(self, tickets: list[TicketCreate]) -> None:
        super().__init__()
        self._tickets = list(tickets)  # Mutable copy

    def compose(self) -> ComposeResult:
        with Vertical(id="approval-container"):
            yield Static("Review Proposed Tickets", id="approval-title")
            yield Static(
                "Press Enter to approve, 't' to toggle type, Escape to cancel",
                id="approval-hint",
            )
            yield DataTable(id="ticket-table")
            with Horizontal(id="approval-buttons"):
                yield Button("Approve All", id="approve", variant="primary")
                yield Button("Refine", id="refine", variant="default")
                yield Button("Cancel", id="cancel", variant="error")

    def on_mount(self) -> None:
        """Set up the data table."""
        table = self.query_one(DataTable)
        table.add_columns("#", "Title", "Type", "Priority")
        table.cursor_type = "row"
        self._refresh_table()
        table.focus()

    def _refresh_table(self) -> None:
        """Refresh the table with current ticket data."""
        table = self.query_one(DataTable)
        table.clear()
        for i, ticket in enumerate(self._tickets, 1):
            type_label = "AUTO" if ticket.ticket_type == TicketType.AUTO else "PAIR"
            priority_label = ticket.priority.label
            max_len = APPROVAL_TITLE_MAX_LENGTH
            title_display = (
                ticket.title[:max_len] + "..." if len(ticket.title) > max_len else ticket.title
            )
            table.add_row(
                str(i),
                title_display,
                type_label,
                priority_label,
            )

    def action_toggle_type(self) -> None:
        """Toggle the ticket type for the selected row."""
        table = self.query_one(DataTable)
        if table.cursor_row is not None and 0 <= table.cursor_row < len(self._tickets):
            idx = table.cursor_row
            ticket = self._tickets[idx]
            new_type = TicketType.PAIR if ticket.ticket_type == TicketType.AUTO else TicketType.AUTO
            # Create new TicketCreate with updated type
            self._tickets[idx] = TicketCreate(
                title=ticket.title,
                description=ticket.description,
                priority=ticket.priority,
                ticket_type=new_type,
                assigned_hat=ticket.assigned_hat,
                status=ticket.status,
                parent_id=ticket.parent_id,
                agent_backend=ticket.agent_backend,
                acceptance_criteria=ticket.acceptance_criteria,
                review_summary=ticket.review_summary,
                checks_passed=ticket.checks_passed,
                session_active=ticket.session_active,
            )
            self._refresh_table()

    def action_approve(self) -> None:
        """Approve all tickets."""
        self.dismiss(self._tickets)

    def action_cancel(self) -> None:
        """Cancel and dismiss."""
        self.dismiss(None)

    @on(DataTable.RowSelected)
    def on_row_selected(self, event: DataTable.RowSelected) -> None:
        """Toggle ticket type when row is selected."""
        self.action_toggle_type()

    @on(Button.Pressed, "#approve")
    def on_approve(self) -> None:
        """Approve all tickets."""
        self.action_approve()

    @on(Button.Pressed, "#refine")
    def on_refine(self) -> None:
        """Open ticket editor for refinement."""
        self.app.push_screen(
            TicketEditorScreen(self._tickets),
            self._on_editor_result,
        )

    def _on_editor_result(self, result: list[TicketCreate] | None) -> None:
        """Handle result from ticket editor screen."""
        if result is not None:
            # Update tickets with edited versions
            self._tickets = result
            self._refresh_table()

    @on(Button.Pressed, "#cancel")
    def on_cancel(self) -> None:
        """Cancel and dismiss."""
        self.action_cancel()
