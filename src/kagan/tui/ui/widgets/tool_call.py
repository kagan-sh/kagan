"""ToolCall widget for displaying tool execution status."""

from __future__ import annotations

import json
import re
from contextlib import suppress
from typing import TYPE_CHECKING, Any

from rich.text import Text
from textual import containers, events, on
from textual.content import Content
from textual.css.query import NoMatches
from textual.reactive import var
from textual.widgets import Markdown, Static

from kagan.core.models.enums import ToolCallStatus
from kagan.tui.ui.utils.helpers import colorize_diff, copy_with_notification

if TYPE_CHECKING:
    from acp.schema import ToolCall as AcpToolCall
    from textual.app import ComposeResult

    ToolCallData = AcpToolCall
else:
    ToolCallData = object

_MARKDOWN_HEURISTIC = re.compile(r"^#{1,6}\s", re.MULTILINE)


class ToolCallHeader(Static):
    """Static row that renders tool title and status."""

    ALLOW_SELECT = False


class TextContent(Static):
    """Plain-text tool payload block."""

    pass


class MarkdownContent(Markdown):
    """Markdown-rendered tool payload block."""

    pass


class ToolContentLabel(Static):
    """Section label used inside expanded tool details."""

    pass


class ToolCall(containers.VerticalGroup):
    """Expandable widget showing tool call status and content."""

    DEFAULT_CLASSES = "tool-call"
    has_content: var[bool] = var(False, toggle_class="-has-content")
    expanded: var[bool] = var(False, toggle_class="-expanded")

    def __init__(
        self, tool_call: ToolCallData, *, id: str | None = None, classes: str | None = None
    ) -> None:
        """Initialize widget with an ACP tool-call payload."""
        self._tool_call = tool_call
        super().__init__(id=id, classes=classes)

    @property
    def tool_call(self) -> ToolCallData:
        """Return the current tool-call payload."""
        return self._tool_call

    @tool_call.setter
    def tool_call(self, tool_call: ToolCallData) -> None:
        """Replace payload and recompose rendered content."""
        self._tool_call = tool_call
        self.refresh(recompose=True)

    def update_status(self, status: str) -> None:
        """Update tool call status and refresh display."""
        self._tool_call = self._tool_call.model_copy(update={"status": status})
        with suppress(NoMatches):
            self.query_one(ToolCallHeader).update(self._header_content)

    def update_tool_call(self, tool_call: ToolCallData) -> None:
        """Apply full tool call update and refresh rendered payload."""
        self._tool_call = tool_call
        self.refresh(recompose=True)

    def compose(self) -> ComposeResult:
        """Render header plus optional content/payload sections."""
        content_list: list[Any] = self._tool_call.content or []
        self.has_content = False
        content_widgets = list(self._compose_content(content_list))
        payload_widgets = list(self._compose_payload())

        header = ToolCallHeader(self._header_content, markup=False)
        header.tooltip = self._tool_call.title
        yield header
        with containers.VerticalGroup(id="tool-content"):
            yield from content_widgets
            yield from payload_widgets

    @property
    def _header_content(self) -> Content:
        title = self._tool_call.title
        status_str = self._tool_call.status or "pending"

        expand_icon = (
            Content("▼ " if self.expanded else "▶ ")
            if self.has_content
            else Content.styled("▶ ", "$text 20%")
        )
        header = Content.assemble(expand_icon, "🔧 ", (title, "$text-success"))

        try:
            status_enum = ToolCallStatus(status_str)
            if status_enum == ToolCallStatus.COMPLETED:
                header += Content.from_markup(f" [$success]{status_enum.icon}")
            else:
                header += Content.assemble(status_enum.icon)
        except ValueError:
            pass
        return header

    def watch_expanded(self) -> None:
        """Refresh header when expand/collapse state changes."""
        with suppress(NoMatches):
            self.query_one(ToolCallHeader).update(self._header_content)

    def watch_has_content(self) -> None:
        """Refresh header when content availability changes."""
        with suppress(NoMatches):
            self.query_one(ToolCallHeader).update(self._header_content)

    @on(events.Click, "ToolCallHeader")
    def on_click_header(self, event: events.Click) -> None:
        """Toggle expand state when header is clicked."""
        event.stop()
        if self.has_content:
            self.expanded = not self.expanded
        else:
            self.app.bell()

    def _compose_content(self, content_list: list[Any]) -> ComposeResult:
        for item in content_list:
            item_type = getattr(item, "type", "")
            if item_type == "content":
                sub_content = item.content
                if getattr(sub_content, "type", "") != "text":
                    continue
                text = sub_content.text or ""
                if not text:
                    continue
                self.has_content = True
                yield from self._compose_text_block(text)
            elif item_type == "diff":
                path = getattr(item, "path", "diff")
                old_text = getattr(item, "old_text", "") or ""
                new_text = getattr(item, "new_text", "") or ""
                diff_text = _build_unified_diff(path, old_text, new_text)
                yield ToolContentLabel("Diff", classes="tool-content-label")
                yield TextContent(colorize_diff(diff_text), markup=True, classes="tool-diff")
                self.has_content = True

    def _compose_payload(self) -> ComposeResult:
        raw_input = getattr(self._tool_call, "raw_input", None)
        raw_output = getattr(self._tool_call, "raw_output", None)
        if raw_input:
            yield ToolContentLabel("Input", classes="tool-content-label")
            yield TextContent(_format_payload(raw_input), markup=False)
            self.has_content = True
        if raw_output:
            yield ToolContentLabel("Output", classes="tool-content-label")
            yield TextContent(_format_payload(raw_output), markup=False)
            self.has_content = True

    def _compose_text_block(self, text: str) -> ComposeResult:
        if "\x1b" in text:
            yield TextContent(Content.from_rich_text(Text.from_ansi(text)))
            return
        if "```" in text or _MARKDOWN_HEURISTIC.search(text):
            yield MarkdownContent(text)
            return
        yield TextContent(text, markup=False)

    async def _on_click(self, event: events.Click) -> None:
        """Handle click events - copy on double-click."""
        if event.chain == 2:
            title = self._tool_call.title
            kind = self._tool_call.kind
            status = self._tool_call.status

            content = f"Tool: {title}"
            if kind:
                content += f" ({kind})"
            if status:
                content += f"\nStatus: {status}"
            copy_with_notification(self.app, content, "Tool call")


def _format_payload(payload: Any) -> str:
    if isinstance(payload, str):
        text = payload.strip()
        if not text:
            return payload
        with suppress(Exception):
            parsed = json.loads(text)
            return json.dumps(parsed, indent=2, ensure_ascii=False)
        return payload

    with suppress(Exception):
        return json.dumps(payload, indent=2, ensure_ascii=False)
    return str(payload)


def _build_unified_diff(path: str, old_text: str, new_text: str) -> str:
    lines = [f"--- a/{path}", f"+++ b/{path}", "@@ -1 +1 @@"]
    lines.extend(f"-{line}" for line in old_text.splitlines())
    lines.extend(f"+{line}" for line in new_text.splitlines())
    return "\n".join(lines)
