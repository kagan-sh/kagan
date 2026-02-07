"""Reusable widgets for agent streaming content."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Horizontal
from textual.widgets import Markdown, Static

from kagan.ui.utils.clipboard import copy_with_notification
from kagan.ui.widgets.streaming_markdown import StreamingMarkdown

if TYPE_CHECKING:
    from textual.app import ComposeResult
    from textual.events import Click


class UserInput(Horizontal):
    """Widget displaying user input with a prompt indicator."""

    DEFAULT_CLASSES = "user-input"

    def __init__(self, content: str) -> None:
        super().__init__()
        self._content = content

    def compose(self) -> ComposeResult:
        yield Static("❯", classes="user-input-prompt")  # noqa: RUF001
        yield Markdown(self._content, classes="user-input-content")

    async def _on_click(self, event: Click) -> None:
        """Handle click events - copy on double-click."""
        if event.chain == 2:
            copy_with_notification(self.app, self._content, "Input")


__all__ = ["StreamingMarkdown", "UserInput"]
