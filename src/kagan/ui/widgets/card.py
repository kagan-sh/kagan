"""TicketCard widget for displaying a Kanban ticket."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from textual.message import Message
from textual.reactive import reactive, var
from textual.widget import Widget
from textual.widgets import Label

from kagan.constants import (
    CARD_BACKEND_MAX_LENGTH,
    CARD_DESC_MAX_LENGTH,
    CARD_HAT_MAX_LENGTH,
    CARD_ID_MAX_LENGTH,
    CARD_REVIEW_MAX_LENGTH,
    CARD_TITLE_LINE_WIDTH,
)
from kagan.database.models import Ticket, TicketPriority, TicketStatus, TicketType

if TYPE_CHECKING:
    from textual import events
    from textual.app import ComposeResult


class TicketCard(Widget):
    """A card widget representing a single ticket on the Kanban board."""

    can_focus = True

    ticket: reactive[Ticket | None] = reactive(None, recompose=True)
    is_agent_active: var[bool] = var(False, toggle_class="agent-active", always_update=True)
    is_session_active: var[bool] = var(False, toggle_class="session-active")
    iteration_info: reactive[str] = reactive("", recompose=True)

    @dataclass
    class Selected(Message):
        ticket: Ticket

    def __init__(self, ticket: Ticket, **kwargs) -> None:
        super().__init__(id=f"card-{ticket.id}", **kwargs)
        self.ticket = ticket
        # Sync session state from ticket
        self.is_session_active = getattr(ticket, "session_active", False)

    def compose(self) -> ComposeResult:
        """Compose the card layout."""
        if self.ticket is None:
            return

        # Line 1-2: Type badge + Title (supports 2 lines)
        type_badge = self._get_type_badge()
        title_lines = self._wrap_title(self.ticket.title, CARD_TITLE_LINE_WIDTH)

        # First line with badge
        first_line = f"{type_badge} {title_lines[0]}" if title_lines else f"{type_badge} Untitled"
        yield Label(first_line, classes="card-title")

        # Second line for long titles (indented to align with first line)
        if len(title_lines) > 1:
            yield Label(f"  {title_lines[1]}", classes="card-title-continued")

        # Description line: Priority icon + description
        priority_class = self._get_priority_class()
        priority_icon = {"LOW": "▽", "MED": "◇", "HIGH": "△"}[self.ticket.priority_label]
        desc = self.ticket.description or "No description"
        desc_text = f"{priority_icon} {self._truncate_title(desc, CARD_DESC_MAX_LENGTH)}"
        yield Label(desc_text, classes=f"card-desc {priority_class}")

        # Meta line: session indicator + backend/hat + AC count + ID + date
        session_indicator = self._get_session_indicator()
        hat = getattr(self.ticket, "assigned_hat", None) or ""
        hat_display = hat[:CARD_HAT_MAX_LENGTH] if hat else ""
        ticket_id = f"#{self.ticket.short_id[:CARD_ID_MAX_LENGTH]}"
        date_str = self.ticket.created_at.strftime("%m.%d.%y")
        backend = getattr(self.ticket, "agent_backend", None) or ""

        # Build meta line with session indicator
        meta_parts = []
        if session_indicator:
            meta_parts.append(session_indicator)
        if backend:
            meta_parts.append(backend[:CARD_BACKEND_MAX_LENGTH])
        elif hat_display:
            meta_parts.append(hat_display)
        # Show acceptance criteria count if present
        ac_count = len(self.ticket.acceptance_criteria) if self.ticket.acceptance_criteria else 0
        if ac_count:
            meta_parts.append(f"[AC:{ac_count}]")
        meta_parts.append(ticket_id)
        meta_parts.append(date_str)

        meta_text = " ".join(meta_parts)
        yield Label(meta_text, classes="card-meta")

        # Review info for REVIEW tickets
        if self.ticket.status == TicketStatus.REVIEW:
            summary = self.ticket.review_summary or "No summary"
            yield Label(
                self._truncate_title(f"Summary: {summary}", CARD_REVIEW_MAX_LENGTH),
                classes="card-review",
            )
            yield Label(self._format_checks_status(), classes="card-checks")

        # Iteration info (if agent is running) - combined with progress
        if self.iteration_info:
            yield Label(self.iteration_info, classes="card-iteration")

    def _get_priority_class(self) -> str:
        """Get CSS class for priority."""
        if self.ticket is None:
            return "low"
        priority = self.ticket.priority
        if isinstance(priority, int):
            priority = TicketPriority(priority)
        return priority.css_class

    def _get_type_badge(self) -> str:
        """Get type badge indicator for ticket type with agent state."""
        if self.ticket is None:
            return "👤"
        ticket_type = self.ticket.ticket_type
        if isinstance(ticket_type, str):
            ticket_type = TicketType(ticket_type)
        if ticket_type == TicketType.AUTO:
            # Show running state for AUTO tickets
            if self.is_agent_active:
                return "🔄"  # Running indicator
            if self.ticket.status == TicketStatus.IN_PROGRESS:
                return "⏳"  # Waiting/pending indicator
            return "⚡"  # Normal AUTO badge
        return "👤"  # PAIR mode (human)

    def _truncate_title(self, title: str, max_length: int) -> str:
        """Truncate title if too long."""
        if len(title) <= max_length:
            return title
        return title[: max_length - 3] + "..."

    def _wrap_title(self, title: str, line_width: int) -> list[str]:
        """Wrap title into multiple lines, respecting word boundaries."""
        if len(title) <= line_width:
            return [title]

        # Try to break at word boundary
        words = title.split()
        lines: list[str] = []
        current_line = ""

        for word in words:
            test_line = f"{current_line} {word}".strip() if current_line else word
            if len(test_line) <= line_width:
                current_line = test_line
            else:
                if current_line:
                    lines.append(current_line)
                # Handle very long words with ternary
                current_line = word[: line_width - 3] + "..." if len(word) > line_width else word

            # Limit to 2 lines
            if len(lines) >= 2:
                break

        if current_line and len(lines) < 2:
            # Truncate final line if needed
            if len(current_line) > line_width:
                current_line = current_line[: line_width - 3] + "..."
            lines.append(current_line)

        return lines if lines else [title[: line_width - 3] + "..."]

    def _get_session_indicator(self) -> str:
        """Get visual indicator for session/agent state."""
        if self.ticket is None:
            return ""

        # Agent actively working - show animated indicator
        if self.is_agent_active:
            return "●"  # Filled circle (will pulse via CSS)

        # tmux session exists but not actively working
        if self.is_session_active or getattr(self.ticket, "session_active", False):
            return "◉"  # Circle with dot (steady state)

        return ""

    def _format_checks_status(self) -> str:
        """Format checks status for review display."""
        if self.ticket is None:
            return "Checks: unknown"
        if self.ticket.checks_passed is True:
            return "Checks: passed"
        if self.ticket.checks_passed is False:
            return "Checks: failed"
        return "Checks: not run"

    def on_click(self, event: events.Click) -> None:
        """Handle click: single-click focuses, double-click opens details."""
        if event.chain == 1:
            # Single click - just focus
            self.focus()
        elif event.chain >= 2 and self.ticket:
            # Double click - open details
            self.post_message(self.Selected(self.ticket))

    def watch_is_agent_active(self, active: bool) -> None:
        """Update card display when agent state changes."""
        # Trigger recompose to update badge
        self.refresh(recompose=True)
