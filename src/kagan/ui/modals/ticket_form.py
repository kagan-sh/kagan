"""Ticket form modal for creating and editing tickets."""

from typing import cast

from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Container, Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, Select, TextArea

from kagan.constants import PRIORITY_LABELS
from kagan.database.models import Ticket, TicketCreate, TicketPriority, TicketStatus, TicketUpdate


class TicketFormModal(ModalScreen[Ticket | TicketCreate | TicketUpdate | None]):
    """Modal screen for creating or editing a ticket."""

    BINDINGS = [
        Binding("escape", "cancel", "Cancel"),
        Binding("ctrl+s", "submit", "Save"),
    ]

    def __init__(self, ticket: Ticket | None = None, **kwargs) -> None:
        super().__init__(**kwargs)
        self.ticket = ticket
        self.is_edit = ticket is not None

    def compose(self) -> ComposeResult:
        """Compose the form layout."""
        title = "Edit Ticket" if self.is_edit else "New Ticket"

        with Container():
            yield Label(title, classes="modal-title")

            with Vertical(classes="form-field"):
                yield Label("Title:", classes="form-label")
                yield Input(
                    value=self.ticket.title if self.ticket else "",
                    placeholder="Enter ticket title...",
                    id="title-input",
                )

            with Vertical(classes="form-field"):
                yield Label("Description:", classes="form-label")
                yield TextArea(
                    self.ticket.description if self.ticket else "",
                    id="description-input",
                )

            with Vertical(classes="form-field"):
                yield Label("Priority:", classes="form-label")
                current_priority = self.ticket.priority if self.ticket else TicketPriority.MEDIUM
                if isinstance(current_priority, int):
                    current_priority = TicketPriority(current_priority)
                yield Select(
                    options=[(label, p.value) for p, label in PRIORITY_LABELS.items()],
                    value=current_priority.value,
                    id="priority-select",
                )

            if not self.is_edit:
                with Vertical(classes="form-field"):
                    yield Label("Status:", classes="form-label")
                    yield Select(
                        options=[
                            ("Backlog", TicketStatus.BACKLOG.value),
                            ("In Progress", TicketStatus.IN_PROGRESS.value),
                            ("Review", TicketStatus.REVIEW.value),
                            ("Done", TicketStatus.DONE.value),
                        ],
                        value=TicketStatus.BACKLOG.value,
                        id="status-select",
                    )

            with Horizontal(classes="button-row"):
                yield Button("Save", variant="primary", id="save-btn")
                yield Button("Cancel", variant="default", id="cancel-btn")

    @on(Button.Pressed, "#save-btn")
    def on_save(self) -> None:
        self.action_submit()

    @on(Button.Pressed, "#cancel-btn")
    def on_cancel(self) -> None:
        self.action_cancel()

    def action_submit(self) -> None:
        """Submit the form."""
        title_input = self.query_one("#title-input", Input)
        description_input = self.query_one("#description-input", TextArea)
        priority_select: Select[int] = self.query_one("#priority-select", Select)

        title = title_input.value.strip()
        if not title:
            self.notify("Title is required", severity="error")
            title_input.focus()
            return

        description = description_input.text

        # Validate priority selection
        priority_value = priority_select.value
        if priority_value is Select.BLANK:
            self.notify("Priority is required", severity="error")
            priority_select.focus()
            return
        priority = TicketPriority(cast("int", priority_value))

        if self.is_edit:
            result = TicketUpdate(
                title=title,
                description=description,
                priority=priority,
            )
        else:
            status_select: Select[str] = self.query_one("#status-select", Select)
            status_value = status_select.value
            if status_value is Select.BLANK:
                self.notify("Status is required", severity="error")
                status_select.focus()
                return
            status = TicketStatus(cast("str", status_value))
            result = TicketCreate(
                title=title,
                description=description,
                priority=priority,
                status=status,
            )

        self.dismiss(result)

    def action_cancel(self) -> None:
        """Cancel and close modal."""
        self.dismiss(None)
