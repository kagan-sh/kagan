"""Floating overlay for peeking at agent status."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Vertical
from textual.widgets import Static

if TYPE_CHECKING:
    from textual.app import ComposeResult


class PeekOverlay(Vertical):
    """Floating overlay showing agent status/scratchpad.

    This widget provides a quick glance at agent state without
    requiring a full modal switch.
    """

    DEFAULT_CSS = """
    PeekOverlay {
        layer: overlay;
        width: 50;
        height: auto;
        max-height: 12;
        background: $surface;
        border: round $primary;
        padding: 1;
        display: none;
        position: absolute;
        overflow-y: auto;
    }

    PeekOverlay.visible {
        display: block;
    }

    PeekOverlay .peek-title {
        text-style: bold;
        color: $primary;
        margin-bottom: 1;
    }

    PeekOverlay .peek-status {
        color: $text;
    }

    PeekOverlay .peek-content {
        color: $text-muted;
    }
    """

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self._title_widget = Static("", classes="peek-title", id="peek-title")
        self._status_widget = Static("", classes="peek-status", id="peek-status")
        self._content_widget = Static("", classes="peek-content", id="peek-content")

    def compose(self) -> ComposeResult:
        yield self._title_widget
        yield self._status_widget
        yield self._content_widget

    def update_content(
        self,
        task_id: str,
        title: str,
        status: str,
        content: str,
    ) -> None:
        """Update the overlay content."""
        self._title_widget.update(f"#{task_id}: {title[:30]}")
        self._status_widget.update(status)
        self._content_widget.update(content[:300] if content else "(No content)")

    def show_at(self, x: int, y: int) -> None:
        """Show overlay at specific position."""
        self.styles.offset = (x, y)
        self.add_class("visible")

    def hide(self) -> None:
        """Hide the overlay."""
        self.remove_class("visible")

    def toggle(self) -> bool:
        """Toggle visibility. Returns True if now visible."""
        if self.has_class("visible"):
            self.hide()
            return False
        return True  # Caller should call show_at() with position
