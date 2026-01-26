"""Ticket details modal for viewing full ticket information."""

from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Label, Rule, Static

from kagan.database.models import Ticket, TicketPriority, TicketStatus
from kagan.ui.modals.actions import ModalAction
from kagan.ui.modals.description_editor import DescriptionEditorModal


class TicketDetailsModal(ModalScreen[ModalAction | None]):
    """Modal screen for viewing ticket details."""

    BINDINGS = [
        Binding("escape", "close", "Close"),
        Binding("e", "edit", "Edit"),
        Binding("d", "delete", "Delete"),
        Binding("f", "expand_description", "Expand Description"),
    ]

    def __init__(self, ticket: Ticket, **kwargs) -> None:
        super().__init__(**kwargs)
        self.ticket = ticket

    def compose(self) -> ComposeResult:
        """Compose the details layout."""
        with Vertical(id="ticket-details-container"):
            yield Label("Ticket Details", classes="modal-title")

            yield Rule(line_style="heavy")

            with Horizontal(classes="badge-row"):
                yield Label(
                    self._get_priority_label(),
                    classes=f"badge {self._get_priority_class()}",
                )
                yield Label(
                    self._format_status(self.ticket.status),
                    classes="badge badge-status",
                )
                if self.ticket.agent_backend:
                    yield Label(
                        self.ticket.agent_backend,
                        classes="badge badge-agent",
                    )

            yield Rule()

            yield Label("Title", classes="section-title")
            yield Static(self.ticket.title, classes="ticket-title")

            yield Rule()

            with Horizontal(classes="description-header"):
                yield Label("Description", classes="section-title")
                yield Static("", classes="header-spacer")
                yield Static("[f] Expand", classes="expand-hint", id="expand-btn")

            description = self.ticket.description or "(No description)"
            yield Static(description, classes="ticket-description", id="description-content")

            yield Rule()

            with Horizontal(classes="meta-row"):
                yield Label(
                    f"Created: {self.ticket.created_at:%Y-%m-%d %H:%M}",
                    classes="ticket-meta",
                )
                yield Static("  |  ", classes="meta-separator")
                yield Label(
                    f"Updated: {self.ticket.updated_at:%Y-%m-%d %H:%M}",
                    classes="ticket-meta",
                )

            yield Rule()

            with Horizontal(classes="button-row"):
                yield Button("[Esc] Close", id="close-btn")
                yield Button("[e] Edit", id="edit-btn")
                yield Button("[d] Delete", variant="error", id="delete-btn")

    def _get_priority_label(self) -> str:
        """Get priority label text."""
        priority = self.ticket.priority
        if isinstance(priority, int):
            priority = TicketPriority(priority)
        return {
            TicketPriority.LOW: "LOW",
            TicketPriority.MEDIUM: "MED",
            TicketPriority.HIGH: "HIGH",
        }.get(priority, "MED")

    def _get_priority_class(self) -> str:
        """Get CSS class for priority badge."""
        priority = self.ticket.priority
        if isinstance(priority, int):
            priority = TicketPriority(priority)
        return {
            TicketPriority.LOW: "badge-priority-low",
            TicketPriority.MEDIUM: "badge-priority-medium",
            TicketPriority.HIGH: "badge-priority-high",
        }.get(priority, "badge-priority-medium")

    def _format_status(self, status: TicketStatus | str) -> str:
        if isinstance(status, str):
            status = TicketStatus(status)
        return status.value.replace("_", " ")

    @on(Button.Pressed, "#edit-btn")
    def on_edit_btn(self) -> None:
        self.action_edit()

    @on(Button.Pressed, "#delete-btn")
    def on_delete_btn(self) -> None:
        self.action_delete()

    @on(Button.Pressed, "#close-btn")
    def on_close_btn(self) -> None:
        self.action_close()

    def action_edit(self) -> None:
        self.dismiss(ModalAction.EDIT)

    def action_delete(self) -> None:
        self.dismiss(ModalAction.DELETE)

    def action_close(self) -> None:
        self.dismiss(None)

    def action_expand_description(self) -> None:
        """Open the description in a full-screen read-only viewer."""
        description = self.ticket.description or ""
        self.app.push_screen(
            DescriptionEditorModal(
                description=description,
                readonly=True,
                title="View Description",
            )
        )
