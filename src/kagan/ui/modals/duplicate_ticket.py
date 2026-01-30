"""Modal for duplicating a ticket with selectable fields."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual import on
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Checkbox, Footer, Input, Label, Rule, Static

from kagan.database.models import TicketCreate, TicketPriority, TicketType
from kagan.keybindings import DUPLICATE_TICKET_BINDINGS, to_textual_bindings

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.database.models import Ticket


class DuplicateTicketModal(ModalScreen[TicketCreate | None]):
    """Modal for duplicating a ticket with selectable fields to copy."""

    BINDINGS = to_textual_bindings(DUPLICATE_TICKET_BINDINGS)

    def __init__(self, source_ticket: Ticket, **kwargs) -> None:
        super().__init__(**kwargs)
        self.source = source_ticket

    def compose(self) -> ComposeResult:
        with Vertical(id="duplicate-container"):
            yield Label("Duplicate Ticket", classes="modal-title")
            yield Static(f"Based on #{self.source.short_id}", classes="source-ref")
            yield Rule()

            # Title (always copied, editable)
            with Vertical(classes="form-field"):
                yield Label("Title:", classes="form-label")
                yield Input(value=self.source.title, id="title-input")

            yield Rule()
            yield Label("Copy fields:", classes="section-title")

            # Checkboxes for optional fields
            with Vertical(classes="checkbox-group"):
                yield Checkbox("Description", value=True, id="copy-description")
                yield Checkbox("Acceptance Criteria", value=False, id="copy-criteria")
                yield Checkbox("Priority", value=False, id="copy-priority")
                yield Checkbox("Ticket Type", value=False, id="copy-type")
                yield Checkbox("Agent Backend", value=False, id="copy-agent")

            yield Rule()
            with Horizontal(classes="button-row"):
                yield Button("[Ctrl+S] Create", variant="primary", id="create-btn")
                yield Button("[Esc] Cancel", id="cancel-btn")

        yield Footer()

    def on_mount(self) -> None:
        """Focus the title input when modal opens."""
        self.query_one("#title-input", Input).focus()

    @on(Button.Pressed, "#create-btn")
    def on_create_btn(self) -> None:
        self.action_create()

    @on(Button.Pressed, "#cancel-btn")
    def on_cancel_btn(self) -> None:
        self.action_cancel()

    def action_create(self) -> None:
        """Create the duplicate ticket with selected fields."""
        title = self.query_one("#title-input", Input).value.strip()
        if not title:
            self.notify("Title is required", severity="error")
            return

        # Build TicketCreate based on checkbox selections
        create_data = TicketCreate(title=title)

        if self.query_one("#copy-description", Checkbox).value:
            create_data = TicketCreate(
                title=title,
                description=self.source.description,
            )

        # Now update with other optional fields
        description = create_data.description

        if self.query_one("#copy-criteria", Checkbox).value:
            create_data = TicketCreate(
                title=title,
                description=description,
                acceptance_criteria=list(self.source.acceptance_criteria),
            )

        if self.query_one("#copy-priority", Checkbox).value:
            priority = self.source.priority
            if isinstance(priority, int):
                priority = TicketPriority(priority)
            create_data = TicketCreate(
                title=title,
                description=description,
                acceptance_criteria=create_data.acceptance_criteria,
                priority=priority,
            )

        if self.query_one("#copy-type", Checkbox).value:
            ticket_type = self.source.ticket_type
            if isinstance(ticket_type, str):
                ticket_type = TicketType(ticket_type)
            create_data = TicketCreate(
                title=title,
                description=description,
                acceptance_criteria=create_data.acceptance_criteria,
                priority=create_data.priority,
                ticket_type=ticket_type,
            )

        if self.query_one("#copy-agent", Checkbox).value:
            create_data = TicketCreate(
                title=title,
                description=description,
                acceptance_criteria=create_data.acceptance_criteria,
                priority=create_data.priority,
                ticket_type=create_data.ticket_type,
                agent_backend=self.source.agent_backend,
            )

        self.dismiss(create_data)

    def action_cancel(self) -> None:
        """Cancel and close the modal."""
        self.dismiss(None)
