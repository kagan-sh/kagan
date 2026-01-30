"""Reusable widgets for agent streaming content."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Horizontal
from textual.widgets import Markdown, Static

from kagan.ui.utils.clipboard import copy_with_notification

if TYPE_CHECKING:
    from textual.app import ComposeResult
    from textual.events import Click
    from textual.widgets.markdown import MarkdownStream


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


class AgentResponse(Markdown):
    """Streaming markdown widget for agent responses."""

    DEFAULT_CLASSES = "agent-response"

    def __init__(self, fragment: str = "") -> None:
        super().__init__(fragment or "")
        self._stream: MarkdownStream | None = None
        self._accumulated_content: str = fragment or ""

    @property
    def stream(self) -> MarkdownStream:
        if self._stream is None:
            self._stream = self.get_stream(self)
        return self._stream

    async def append_fragment(self, fragment: str) -> None:
        """Append a text fragment to the streaming response."""
        self.loading = False
        self._accumulated_content += fragment
        await self.stream.write(fragment)

    async def _on_click(self, event: Click) -> None:
        """Handle click events - copy on double-click."""
        if event.chain == 2:
            copy_with_notification(self.app, self._accumulated_content, "Response")


class AgentThought(Markdown):
    """Streaming markdown widget for agent thinking/reasoning."""

    DEFAULT_CLASSES = "agent-thought"

    def __init__(self, fragment: str = "") -> None:
        super().__init__(fragment or "")
        self._stream: MarkdownStream | None = None
        self._accumulated_content: str = fragment or ""

    @property
    def stream(self) -> MarkdownStream:
        if self._stream is None:
            self._stream = self.get_stream(self)
        return self._stream

    async def append_fragment(self, fragment: str) -> None:
        """Append a thought fragment."""
        self.loading = False
        self._accumulated_content += fragment
        await self.stream.write(fragment)
        self.scroll_end(animate=False)

    async def _on_click(self, event: Click) -> None:
        """Handle click events - copy on double-click."""
        if event.chain == 2:
            copy_with_notification(self.app, self._accumulated_content, "Thought")
