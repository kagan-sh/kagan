"""TicketCard widget for displaying a Kanban ticket."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from textual.message import Message
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Label

from kagan.constants import COLUMN_ORDER
from kagan.database.models import Ticket, TicketPriority, TicketStatus

if TYPE_CHECKING:
    from textual import events
    from textual.app import ComposeResult
    from textual.timer import Timer


class TicketCard(Widget):
    """A card widget representing a single ticket on the Kanban board."""

    can_focus = True

    ticket: reactive[Ticket | None] = reactive(None)
    is_agent_active: reactive[bool] = reactive(False)
    iteration_info: reactive[str] = reactive("")
    _dragging: bool = False
    _drag_start_x: int = 0
    _pulse_timer: Timer | None = None

    @dataclass
    class Selected(Message):
        ticket: Ticket

    @dataclass
    class MoveRequested(Message):
        ticket: Ticket
        forward: bool = True

    @dataclass
    class EditRequested(Message):
        ticket: Ticket

    @dataclass
    class DeleteRequested(Message):
        ticket: Ticket

    @dataclass
    class DragMove(Message):
        ticket: Ticket
        target_status: TicketStatus | None

    def __init__(self, ticket: Ticket, **kwargs) -> None:
        super().__init__(id=f"card-{ticket.id}", **kwargs)
        self.ticket = ticket

    def compose(self) -> ComposeResult:
        """Compose the card layout."""
        if self.ticket is None:
            return

        # Line 1: Title (truncated to fit)
        yield Label(self._truncate_title(self.ticket.title, 18), classes="card-title")

        # Line 2: Priority icon + description
        priority_class = self._get_priority_class()
        priority_icon = {"LOW": "▽", "MED": "◇", "HIGH": "△"}[self.ticket.priority_label]
        desc = self.ticket.description or "No description"
        desc_text = f"{priority_icon} {self._truncate_title(desc, 15)}"
        yield Label(desc_text, classes=f"card-desc {priority_class}")

        # Line 3: hat + ID + date
        hat = getattr(self.ticket, "assigned_hat", None) or ""
        hat_display = hat[:8] if hat else ""  # Truncate hat to 8 chars
        ticket_id = f"#{self.ticket.short_id[:4]}"  # Short 4-char ID
        date_str = self.ticket.created_at.strftime("%m/%d")

        # Build meta line with spacing
        if hat_display:
            meta_text = f"{hat_display}  {ticket_id} {date_str}"
        else:
            meta_text = f"{ticket_id} {date_str}"

        yield Label(meta_text, classes="card-meta")

        # Line 4: Iteration info (if agent is running)
        if self.iteration_info:
            yield Label(self.iteration_info, classes="card-iteration")

    def _get_priority_class(self) -> str:
        """Get CSS class for priority."""
        if self.ticket is None:
            return "low"
        priority = self.ticket.priority
        if isinstance(priority, TicketPriority):
            priority = priority.value
        return {0: "low", 1: "medium", 2: "high"}.get(priority, "medium")

    def _truncate_title(self, title: str, max_length: int) -> str:
        """Truncate title if too long."""
        if len(title) <= max_length:
            return title
        return title[: max_length - 3] + "..."

    def on_mouse_down(self, event: events.MouseDown) -> None:
        """Start potential drag operation."""
        self._drag_start_x = event.screen_x
        self.capture_mouse()

    def on_mouse_up(self, event: events.MouseUp) -> None:
        """End drag operation or handle click."""
        was_dragging = self._dragging
        self._dragging = False
        self.release_mouse()
        self.remove_class("dragging")

        if was_dragging and self.ticket:
            # Calculate which column we're over based on screen position
            target_status = self._get_target_status(event.screen_x)
            if target_status and target_status != self.ticket.status:
                self.post_message(self.DragMove(self.ticket, target_status))
        elif self.ticket:
            # Was a click, not a drag
            self.post_message(self.Selected(self.ticket))

    def on_mouse_move(self, event: events.MouseMove) -> None:
        """Handle mouse movement during drag."""
        if event.button != 0:
            return
        # Start drag if moved more than 5 pixels horizontally
        if not self._dragging and abs(event.screen_x - self._drag_start_x) > 5:
            self._dragging = True
            self.add_class("dragging")

    def _get_target_status(self, screen_x: int) -> TicketStatus | None:
        """Determine target column based on screen X position."""
        try:
            screen_width = self.app.size.width
            column_width = screen_width // 4
            column_index = min(3, max(0, screen_x // column_width))
            return COLUMN_ORDER[column_index]
        except Exception:
            return None

    def watch_is_agent_active(self, active: bool) -> None:
        """React to agent active state changes."""
        if active:
            self.add_class("agent-active")
            self._pulse_timer = self.set_interval(0.6, self._toggle_pulse)
        else:
            if self._pulse_timer is not None:
                self._pulse_timer.stop()
                self._pulse_timer = None
            self.remove_class("agent-active")
            self.remove_class("agent-pulse")

    def _toggle_pulse(self) -> None:
        """Toggle the pulse class for animation effect."""
        self.toggle_class("agent-pulse")
