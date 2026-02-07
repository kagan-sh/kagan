"""Offline banner widget for agent unavailability."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Horizontal
from textual.message import Message
from textual.widgets import Button, Label, Static

if TYPE_CHECKING:
    from textual.app import ComposeResult


class OfflineBanner(Static):
    """Banner shown when agent is unavailable."""

    class Retry(Message):
        """User clicked retry."""

    class Dismissed(Message):
        """User dismissed banner."""

    def __init__(self, message: str | None = None) -> None:
        super().__init__()
        self._message = message or "Agent unavailable"

    def compose(self) -> ComposeResult:
        yield Label("⚠️  OFFLINE MODE", id="offline-title")
        yield Label(self._message, id="offline-message")
        with Horizontal(id="offline-actions"):
            yield Button("Retry", id="offline-retry", variant="primary")
            yield Button("Dismiss", id="offline-dismiss")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "offline-retry":
            self.post_message(self.Retry())
        elif event.button.id == "offline-dismiss":
            self.post_message(self.Dismissed())
            self.remove()
