"""Ticket editor screen for refining proposed tickets from planner."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

from textual import on
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, Select, TabbedContent, TabPane, TextArea

from kagan.database.models import TicketCreate, TicketPriority, TicketType
from kagan.keybindings import TICKET_EDITOR_BINDINGS, to_textual_bindings

if TYPE_CHECKING:
    from textual.app import ComposeResult


class TicketEditorScreen(ModalScreen[list[TicketCreate] | None]):
    """Edit proposed tickets before approval.

    Returns:
        list[TicketCreate]: Edited tickets
        None: User cancelled
    """

    BINDINGS = to_textual_bindings(TICKET_EDITOR_BINDINGS)

    def __init__(self, tickets: list[TicketCreate]) -> None:
        super().__init__()
        self._tickets = list(tickets)  # Mutable copy

    def compose(self) -> ComposeResult:
        with Vertical(id="ticket-editor-container"):
            with TabbedContent(id="ticket-tabs"):
                for i, ticket in enumerate(self._tickets, 1):
                    with TabPane(f"Ticket {i}", id=f"ticket-{i}"):
                        with Vertical(classes="ticket-form"):
                            yield Input(
                                value=ticket.title,
                                placeholder="Title",
                                id=f"title-{i}",
                                classes="ticket-input",
                            )
                            yield TextArea(
                                text=ticket.description or "",
                                id=f"description-{i}",
                                classes="ticket-textarea",
                            )
                            yield Select(
                                options=[
                                    ("Low", TicketPriority.LOW.value),
                                    ("Medium", TicketPriority.MEDIUM.value),
                                    ("High", TicketPriority.HIGH.value),
                                ],
                                value=ticket.priority.value,
                                id=f"priority-{i}",
                                classes="ticket-select",
                            )
                            yield Select(
                                options=[
                                    ("AUTO - AI completes autonomously", TicketType.AUTO.value),
                                    ("PAIR - Human collaboration needed", TicketType.PAIR.value),
                                ],
                                value=ticket.ticket_type.value,
                                id=f"type-{i}",
                                classes="ticket-select",
                            )
                            yield Label(
                                "Acceptance Criteria (one per line):", classes="ticket-label"
                            )
                            ac_text = (
                                "\n".join(ticket.acceptance_criteria)
                                if ticket.acceptance_criteria
                                else ""
                            )
                            yield TextArea(
                                text=ac_text,
                                id=f"ac-{i}",
                                classes="ticket-textarea",
                            )
            yield Button("Finish Editing", id="finish-btn", variant="primary")

    def on_mount(self) -> None:
        """Focus the first input field."""
        if self._tickets:
            first_input = self.query_one("#title-1", Input)
            first_input.focus()

    def _collect_edited_tickets(self) -> list[TicketCreate]:
        """Collect all edited tickets from the form fields."""
        edited_tickets: list[TicketCreate] = []

        for i, original in enumerate(self._tickets, 1):
            title_input = self.query_one(f"#title-{i}", Input)
            description_input = self.query_one(f"#description-{i}", TextArea)
            priority_select: Select[int] = self.query_one(f"#priority-{i}", Select)
            type_select: Select[str] = self.query_one(f"#type-{i}", Select)
            ac_input = self.query_one(f"#ac-{i}", TextArea)

            # Parse acceptance criteria from TextArea
            ac_lines = ac_input.text.strip().split("\n") if ac_input.text.strip() else []
            acceptance_criteria = [line.strip() for line in ac_lines if line.strip()]

            # Get values with fallbacks
            title = title_input.value.strip() or original.title
            description = description_input.text or original.description

            priority_value = priority_select.value
            if priority_value is Select.BLANK:
                priority = original.priority
            else:
                priority = TicketPriority(cast("int", priority_value))

            type_value = type_select.value
            if type_value is Select.BLANK:
                ticket_type = original.ticket_type
            else:
                ticket_type = TicketType(cast("str", type_value))

            edited_tickets.append(
                TicketCreate(
                    title=title,
                    description=description,
                    priority=priority,
                    ticket_type=ticket_type,
                    assigned_hat=original.assigned_hat,
                    status=original.status,
                    parent_id=original.parent_id,
                    agent_backend=original.agent_backend,
                    acceptance_criteria=acceptance_criteria,
                    review_summary=original.review_summary,
                    checks_passed=original.checks_passed,
                    session_active=original.session_active,
                )
            )

        return edited_tickets

    def action_finish(self) -> None:
        """Finish editing and return the edited tickets."""
        edited_tickets = self._collect_edited_tickets()
        self.dismiss(edited_tickets)

    def action_cancel(self) -> None:
        """Cancel and dismiss without changes."""
        self.dismiss(None)

    @on(Button.Pressed, "#finish-btn")
    def on_finish(self) -> None:
        """Handle finish button press."""
        self.action_finish()
