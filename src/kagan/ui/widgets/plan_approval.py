"""Inline plan approval widget for the planner screen."""

from __future__ import annotations

from typing import TYPE_CHECKING, ClassVar

from textual.binding import Binding, BindingType
from textual.containers import Horizontal, Vertical, VerticalGroup
from textual.message import Message
from textual.widgets import Button, Static

from kagan.database.models import TicketType

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.database.models import Ticket


class PlanApprovalWidget(VerticalGroup):
    """Inline widget for reviewing and approving generated plan tickets."""

    DEFAULT_CLASSES = "plan-approval"

    BINDINGS: ClassVar[list[BindingType]] = [
        Binding("a", "approve", "Approve"),
        Binding("e", "edit", "Edit"),
        Binding("d", "dismiss", "Dismiss"),
        Binding("escape", "dismiss", "Dismiss"),
    ]

    class Approved(Message):
        """Message posted when the plan is approved."""

        def __init__(self, tickets: list[Ticket]) -> None:
            super().__init__()
            self.tickets = tickets

    class EditRequested(Message):
        """Message posted when user wants to edit tickets."""

        def __init__(self, tickets: list[Ticket]) -> None:
            super().__init__()
            self.tickets = tickets

    class Dismissed(Message):
        """Message posted when the plan is dismissed."""

    def __init__(
        self,
        tickets: list[Ticket],
        *,
        id: str | None = None,
        classes: str | None = None,
    ) -> None:
        super().__init__(id=id, classes=classes)
        self._tickets = tickets
        self._selected_index = 0

    def compose(self) -> ComposeResult:
        n = len(self._tickets)
        yield Static(
            f"📋 Generated Plan ({n} ticket{'s' if n != 1 else ''})", classes="plan-header"
        )
        yield Static("Use ↑/↓ to navigate, Enter to preview", classes="plan-hint")

        with Vertical(id="ticket-list"):
            for i, ticket in enumerate(self._tickets):
                yield self._make_ticket_row(ticket, i)

        with Horizontal(id="approval-buttons"):
            yield Button("[a] Approve", id="btn-approve", variant="success")
            yield Button("[e] Edit", id="btn-edit", variant="warning")
            yield Button("[d] Dismiss", id="btn-dismiss", variant="error")

    def _make_ticket_row(self, ticket: Ticket, index: int) -> Static:
        """Create a row displaying a single ticket."""
        # Type badge
        type_badge = "⚡ AUTO" if ticket.ticket_type == TicketType.AUTO else "👤 PAIR"

        # Priority badge
        priority_label = ticket.priority.label.upper()

        # Title (truncate if needed)
        title = ticket.title
        if len(title) > 60:
            title = title[:57] + "..."

        row_text = f"  {type_badge}  {title}  [{priority_label}]"
        classes = f"ticket-row priority-{ticket.priority.css_class}"
        if index == self._selected_index:
            classes += " selected"
        return Static(row_text, classes=classes, id=f"ticket-row-{index}")

    def on_mount(self) -> None:
        """Focus self and update selection."""
        self._update_selection()
        self.focus()

    def _update_selection(self) -> None:
        """Update visual selection state."""
        for i in range(len(self._tickets)):
            row = self.query_one(f"#ticket-row-{i}", Static)
            if i == self._selected_index:
                row.add_class("selected")
            else:
                row.remove_class("selected")

    def on_key(self, event) -> None:
        """Handle keyboard navigation."""
        if event.key in ("up", "k"):
            if self._selected_index > 0:
                self._selected_index -= 1
                self._update_selection()
            event.stop()
        elif event.key in ("down", "j"):
            if self._selected_index < len(self._tickets) - 1:
                self._selected_index += 1
                self._update_selection()
            event.stop()
        elif event.key == "enter":
            # Show preview of selected ticket
            self._show_preview()
            event.stop()

    def _show_preview(self) -> None:
        """Show preview of the selected ticket."""
        if 0 <= self._selected_index < len(self._tickets):
            ticket = self._tickets[self._selected_index]
            # Build preview text
            type_str = (
                "AUTO (AI autonomous)"
                if ticket.ticket_type == TicketType.AUTO
                else "PAIR (human collaboration)"
            )
            ac_text = (
                "\n".join(f"  • {c}" for c in ticket.acceptance_criteria)
                if ticket.acceptance_criteria
                else "  (none)"
            )

            preview = f"""**{ticket.title}**

**Type:** {type_str}
**Priority:** {ticket.priority.label}

**Description:**
{ticket.description or "(no description)"}

**Acceptance Criteria:**
{ac_text}
"""
            self.notify(preview, title=f"Ticket {self._selected_index + 1} Preview", timeout=10)

    def action_approve(self) -> None:
        """Approve the plan and post Approved message."""
        self.post_message(self.Approved(self._tickets))
        self.remove()

    def action_edit(self) -> None:
        """Request to edit tickets before approval."""
        self.post_message(self.EditRequested(self._tickets))
        self.remove()

    def action_dismiss(self) -> None:
        """Dismiss the plan and post Dismissed message."""
        self.post_message(self.Dismissed())
        self.remove()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        event.stop()
        if event.button.id == "btn-approve":
            self.action_approve()
        elif event.button.id == "btn-edit":
            self.action_edit()
        elif event.button.id == "btn-dismiss":
            self.action_dismiss()
