"""Streaming markdown widget for agent content display."""

from __future__ import annotations

import asyncio
from contextlib import suppress
from typing import TYPE_CHECKING

from textual.containers import Horizontal
from textual.widgets import Markdown, Static
from textual.widgets.markdown import MarkdownStream  # noqa: TC002 (used at runtime)

from kagan.core.models.enums import StreamRole  # noqa: TC001 (used at runtime for type)
from kagan.tui.ui.utils.helpers import copy_with_notification

if TYPE_CHECKING:
    from textual.app import ComposeResult
    from textual.events import Click


class StreamingMarkdown(Markdown):
    """Markdown widget that supports streaming content updates."""

    def __init__(
        self,
        role: StreamRole,
        **kwargs,
    ) -> None:
        """Initialize StreamingMarkdown widget.

        Args:
            role: The role type (RESPONSE or THOUGHT) for CSS classification
            **kwargs: Additional arguments passed to Markdown
        """
        super().__init__("", **kwargs)
        self._role = role
        self._accumulated_content = ""
        self._stream: MarkdownStream | None = None
        self.add_class(f"agent-{role}")

    @property
    def role(self) -> StreamRole:
        """Get the widget's role."""
        return self._role

    @property
    def content(self) -> str:
        return self._accumulated_content

    @property
    def stream(self) -> MarkdownStream:
        """Get the Markdown stream used for incremental updates."""
        if self._stream is None:
            self._stream = self.get_stream(self)
        return self._stream

    async def append_content(self, text: str) -> None:
        """Append text to the accumulated content and update the display.

        Args:
            text: Text fragment to append
        """
        self._accumulated_content += text
        await self.stream.write(text)

    async def stop_stream(self) -> None:
        """Stop the active Markdown stream task if present."""
        stream = self._stream
        self._stream = None
        if stream is None:
            return
        with suppress(asyncio.CancelledError):
            await stream.stop()

    async def on_unmount(self) -> None:
        """Ensure stream workers are cancelled before widget teardown."""
        await self.stop_stream()

    def clear(self) -> None:
        self._accumulated_content = ""
        stream = self._stream
        self._stream = None
        if stream is not None:
            with suppress(RuntimeError):
                asyncio.get_running_loop().create_task(self._stop_stream_background(stream))
        self.update("")

    async def _stop_stream_background(self, stream: MarkdownStream) -> None:
        with suppress(asyncio.CancelledError):
            await stream.stop()


class UserInput(Horizontal):
    DEFAULT_CLASSES = "user-input"

    def __init__(self, content: str) -> None:
        super().__init__()
        self._content = content

    def compose(self) -> ComposeResult:
        yield Static("❯", classes="user-input-prompt")  # noqa: RUF001
        yield Markdown(self._content, classes="user-input-content")

    async def _on_click(self, event: Click) -> None:
        if event.chain == 2:
            copy_with_notification(self.app, self._content, "Input")


__all__ = ["StreamingMarkdown", "UserInput"]
