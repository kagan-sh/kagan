"""Streaming markdown widget for agent content display."""

from __future__ import annotations

from typing import Literal

from textual.widgets import Markdown
from textual.widgets.markdown import MarkdownStream


class StreamingMarkdown(Markdown):
    """Markdown widget that supports streaming content updates."""

    def __init__(
        self,
        role: Literal["response", "thought"],
        **kwargs,
    ) -> None:
        """Initialize StreamingMarkdown widget.

        Args:
            role: The role type ("response" or "thought") for CSS classification
            **kwargs: Additional arguments passed to Markdown
        """
        super().__init__("", **kwargs)
        self._role = role
        self._accumulated_content = ""
        self._stream: MarkdownStream | None = None
        self.add_class(f"agent-{role}")

    @property
    def role(self) -> Literal["response", "thought"]:
        """Get the widget's role."""
        return self._role  # type: ignore[return-value]

    @property
    def content(self) -> str:
        """Get the accumulated content."""
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

    def clear(self) -> None:
        """Clear all accumulated content."""
        self._accumulated_content = ""
        self._stream = None
        self.update("")
