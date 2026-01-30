"""ToolCall widget for displaying tool execution status."""

from __future__ import annotations

from contextlib import suppress
from typing import TYPE_CHECKING, Any

from textual import containers, events, on
from textual.content import Content
from textual.css.query import NoMatches
from textual.reactive import var
from textual.widgets import Static

from kagan.ui.utils.clipboard import copy_with_notification

if TYPE_CHECKING:
    from textual.app import ComposeResult

ToolCallData = dict[str, Any]  # Contains: id, title, kind, status, content


class ToolCallHeader(Static):
    ALLOW_SELECT = False


class TextContent(Static):
    pass


class ToolCall(containers.VerticalGroup):
    """Expandable widget showing tool call status and content."""

    DEFAULT_CLASSES = "tool-call"
    has_content: var[bool] = var(False, toggle_class="-has-content")
    expanded: var[bool] = var(False, toggle_class="-expanded")

    def __init__(
        self, tool_call: ToolCallData, *, id: str | None = None, classes: str | None = None
    ) -> None:
        self._tool_call = tool_call
        super().__init__(id=id, classes=classes)

    @property
    def tool_call(self) -> ToolCallData:
        return self._tool_call

    @tool_call.setter
    def tool_call(self, tool_call: ToolCallData) -> None:
        self._tool_call = tool_call
        self.refresh(recompose=True)

    def update_status(self, status: str) -> None:
        """Update tool call status and refresh display."""
        self._tool_call["status"] = status
        with suppress(NoMatches):
            self.query_one(ToolCallHeader).update(self._header_content)

    def compose(self) -> ComposeResult:
        content_list: list[dict[str, Any]] = self._tool_call.get("content") or []
        self.has_content = False
        content_widgets = list(self._compose_content(content_list))

        header = ToolCallHeader(self._header_content, markup=False)
        header.tooltip = self._tool_call.get("title", "Tool Call")
        yield header
        with containers.VerticalGroup(id="tool-content"):
            yield from content_widgets

    @property
    def _header_content(self) -> Content:
        title = self._tool_call.get("title", "Tool Call")
        status = self._tool_call.get("status", "pending")

        expand_icon = (
            Content("▼ " if self.expanded else "▶ ")
            if self.has_content
            else Content.styled("▶ ", "$text 20%")
        )
        header = Content.assemble(expand_icon, "🔧 ", (title, "$text-success"))

        status_icons = {"pending": " ⏲", "in_progress": " ⋯", "completed": " ✔", "failed": " ❌"}
        if status in status_icons:
            if status == "completed":
                header += Content.from_markup(f" [$success]{status_icons[status]}")
            else:
                header += Content.assemble(status_icons[status])
        return header

    def watch_expanded(self) -> None:
        with suppress(NoMatches):
            self.query_one(ToolCallHeader).update(self._header_content)

    def watch_has_content(self) -> None:
        with suppress(NoMatches):
            self.query_one(ToolCallHeader).update(self._header_content)

    @on(events.Click, "ToolCallHeader")
    def on_click_header(self, event: events.Click) -> None:
        event.stop()
        if self.has_content:
            self.expanded = not self.expanded
        else:
            self.app.bell()

    def _compose_content(self, content_list: list[dict[str, Any]]) -> ComposeResult:
        for item in content_list:
            if item.get("type") == "content":
                sub_content = item.get("content", {})
                if sub_content.get("type") == "text" and (text := sub_content.get("text", "")):
                    yield TextContent(text, markup=False)
                    self.has_content = True

    async def _on_click(self, event: events.Click) -> None:
        """Handle click events - copy on double-click."""
        if event.chain == 2:
            title = self._tool_call.get("title", "Tool Call")
            kind = self._tool_call.get("kind")
            status = self._tool_call.get("status")

            content = f"Tool: {title}"
            if kind:
                content += f" ({kind})"
            if status:
                content += f"\nStatus: {status}"
            copy_with_notification(self.app, content, "Tool call")
