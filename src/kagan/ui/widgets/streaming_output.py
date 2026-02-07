"""Streaming output container for agent conversation display."""

from __future__ import annotations

import re
from collections import OrderedDict
from typing import TYPE_CHECKING, Any, Literal, cast
from uuid import uuid4

from acp.schema import PlanEntry
from acp.schema import ToolCall as AcpToolCall
from acp.schema import ToolCallUpdate as AcpToolCallUpdate
from pydantic import ValidationError
from textual.containers import VerticalScroll
from textual.widgets import Rule, Static

from kagan.limits import MAX_TOOL_CALLS
from kagan.ui.utils.animation import WAVE_FRAMES, WAVE_INTERVAL_MS
from kagan.ui.widgets.agent_content import StreamingMarkdown, UserInput
from kagan.ui.widgets.permission_prompt import PermissionPrompt
from kagan.ui.widgets.plan_approval import PlanApprovalWidget
from kagan.ui.widgets.plan_display import PlanDisplay
from kagan.ui.widgets.tool_call import ToolCall

if TYPE_CHECKING:
    import asyncio

    from acp.schema import PermissionOption
    from textual.app import ComposeResult
    from textual.widget import Widget

    from kagan.acp.messages import Answer
    from kagan.core.models.entities import Task
    from kagan.ui.widgets.tool_call import ToolCallStatus

# Phase state machine
StreamPhase = Literal["idle", "thinking", "streaming", "complete"]

# Regex to strip plan/todos XML blocks from response text
XML_BLOCK_PATTERN = re.compile(r"<(todos|plan)>.*?</\1>", re.DOTALL | re.IGNORECASE)
# Pattern to detect start of potential XML block (for buffering during streaming)
XML_PARTIAL_START = re.compile(r"<(todos|plan)", re.IGNORECASE)


class ThinkingIndicator(Static):
    """Animated thinking indicator with wave animation."""

    def __init__(self, label: str = "Thinking...", **kwargs) -> None:
        self._label = label
        super().__init__(f"{WAVE_FRAMES[0]} {label}", **kwargs)
        self._frame_index = 0
        self._timer = None

    def on_mount(self) -> None:
        """Start animation when mounted."""
        self._timer = self.set_interval(WAVE_INTERVAL_MS / 1000, self._next_frame, pause=False)

    def on_unmount(self) -> None:
        """Stop animation when unmounted."""
        if self._timer:
            self._timer.stop()

    def _next_frame(self) -> None:
        """Advance to the next animation frame."""
        self._frame_index = (self._frame_index + 1) % len(WAVE_FRAMES)
        self.update(f"{WAVE_FRAMES[self._frame_index]} {self._label}")


class StreamingOutput(VerticalScroll):
    """Container for streaming agent conversation content."""

    def __init__(self, *, id: str | None = None, classes: str | None = None) -> None:
        super().__init__(id=id, classes=classes)
        self._agent_response: StreamingMarkdown | None = None
        self._agent_thought: StreamingMarkdown | None = None
        self._tool_calls: OrderedDict[str, ToolCall] = OrderedDict()
        self._plan_display: PlanDisplay | None = None
        self._thinking_indicator: ThinkingIndicator | None = None
        self._phase: StreamPhase = "idle"
        self._xml_buffer: str = ""  # Buffer for potential partial XML tags

    @property
    def phase(self) -> StreamPhase:
        return self._phase

    def set_phase(self, phase: StreamPhase) -> None:
        self._phase = phase

    def compose(self) -> ComposeResult:
        yield from ()

    async def post_user_input(self, text: str) -> UserInput:
        """Post user input as a separate widget."""
        widget = UserInput(text)
        await self.mount(widget)
        self._scroll_to_end()
        return widget

    async def post_thinking_indicator(self) -> ThinkingIndicator:
        """Mount a thinking indicator, removed when streaming starts."""
        await self._remove_thinking_indicator()
        self._thinking_indicator = ThinkingIndicator(classes="thinking-indicator")
        await self.mount(self._thinking_indicator)
        self._scroll_to_end()
        self._phase = "thinking"
        return self._thinking_indicator

    async def _remove_thinking_indicator(self) -> None:
        """Remove thinking indicator if present."""
        if self._thinking_indicator is not None:
            await self._thinking_indicator.remove()
            self._thinking_indicator = None

    async def post_response(self, fragment: str = "") -> StreamingMarkdown:
        """Get or create agent response widget. Resets thought state."""
        await self._remove_thinking_indicator()
        self._agent_thought = None
        self._phase = "streaming"

        # Filter out XML blocks from fragment (with buffering for partial tags)
        if fragment:
            fragment = self._filter_xml_content(fragment)

        if self._agent_response is None:
            self._agent_response = StreamingMarkdown(role="response")
            await self.mount(self._agent_response)
            if fragment:
                await self._agent_response.append_content(fragment)
        elif fragment:
            await self._agent_response.append_content(fragment)
        self._scroll_to_end()
        return self._agent_response

    def _filter_xml_content(self, fragment: str) -> str:
        """Filter XML blocks from fragment, buffering partial tags.

        This handles streaming where XML tags may arrive split across fragments.
        """
        # Combine buffer with new fragment
        combined = self._xml_buffer + fragment
        self._xml_buffer = ""

        # First, remove any complete XML blocks
        combined = XML_BLOCK_PATTERN.sub("", combined)

        # Check if we're inside an incomplete XML block
        # Look for opening tag without matching closing tag
        match = XML_PARTIAL_START.search(combined)
        if match:
            tag_name = match.group(1).lower()
            close_tag = f"</{tag_name}>"

            # Check if there's a closing tag after the opening
            close_pos = combined.lower().find(close_tag, match.start())
            if close_pos == -1:
                # No closing tag found - we're in a partial block
                # Buffer everything from the start of the XML tag
                safe_content = combined[: match.start()]
                self._xml_buffer = combined[match.start() :]
                return safe_content

        return combined

    def flush_xml_buffer(self) -> str:
        """Flush any remaining XML buffer content (call at end of stream).

        Returns any non-XML content that was buffered.
        """
        if not self._xml_buffer:
            return ""

        # At end of stream, filter complete blocks and return rest
        content = XML_BLOCK_PATTERN.sub("", self._xml_buffer)
        self._xml_buffer = ""

        # If it still looks like an incomplete XML block, discard it
        if XML_PARTIAL_START.match(content):
            return ""

        return content

    async def post_thought(self, fragment: str) -> StreamingMarkdown:
        """Get or create agent thought widget."""
        await self._remove_thinking_indicator()
        if self._agent_thought is None:
            self._agent_thought = StreamingMarkdown(role="thought")
            await self.mount(self._agent_thought)
            await self._agent_thought.append_content(fragment)
        else:
            await self._agent_thought.append_content(fragment)
        self._scroll_to_end()
        return self._agent_thought

    async def post_tool_call(self, tool_id: str, title: str, kind: str = "") -> ToolCall:
        """Post a tool call notification.

        Idempotent: returns existing widget if tool_id already exists.
        """
        await self._remove_thinking_indicator()
        self._agent_response = None
        self._agent_thought = None

        # Generate unique ID if tool_id is unknown/empty
        if not tool_id or tool_id == "unknown":
            tool_id = f"auto-{uuid4().hex[:8]}"
        elif tool_id in self._tool_calls:
            # Avoid duplicate widget IDs for repeated tool call messages.
            # Return existing widget - this makes the method idempotent.
            return self._tool_calls[tool_id]

        # Double-check DOM for existing widget (defensive against race conditions)
        widget_id = f"tool-{tool_id}"
        try:
            existing = self.query_one(f"#{widget_id}", ToolCall)
            # Widget exists in DOM but not in our tracking dict - add it back
            self._tool_calls[tool_id] = existing
            return existing
        except Exception:
            pass  # Widget doesn't exist in DOM, proceed to create

        tool_data = AcpToolCall(
            toolCallId=tool_id,
            title=title,
            kind=cast("Any", kind) if kind else None,
            status="pending",
        )
        widget = ToolCall(tool_data, id=widget_id)
        self._tool_calls[tool_id] = widget

        # Evict oldest entries if over limit
        while len(self._tool_calls) > MAX_TOOL_CALLS:
            _old_id, old_widget = self._tool_calls.popitem(last=False)
            old_widget.remove()  # Remove from DOM (sync remove is fine)

        await self.mount(widget)
        self._scroll_to_end()
        return widget

    def update_tool_status(self, tool_id: str, status: str) -> None:
        """Update a tool call's status."""
        if tool_id in self._tool_calls:
            self._tool_calls[tool_id].update_status(cast("ToolCallStatus", status))

    async def post_note(self, text: str, classes: str = "") -> Widget:
        """Post a simple text note."""
        widget = Static(text, classes=f"streaming-note {classes}".strip())
        await self.mount(widget)
        self._scroll_to_end()
        return widget

    async def post_plan(self, entries: list[PlanEntry] | list[dict[str, object]]) -> PlanDisplay:
        """Display agent plan entries. Updates existing if present."""
        self._agent_thought = None
        # Do NOT reset _agent_response here - causes line overwriting bug
        normalized = _coerce_plan_entries(entries)

        if self._plan_display is not None:
            # Update existing plan display in-place
            self._plan_display.update_entries(normalized)
        else:
            self._plan_display = PlanDisplay(normalized, classes="plan-display")
            await self.mount(self._plan_display)

        self._scroll_to_end()
        return self._plan_display

    async def post_permission_request(
        self,
        options: list[PermissionOption],
        tool_call: AcpToolCall | AcpToolCallUpdate,
        result_future: asyncio.Future[Answer],
        timeout: float = 300.0,
    ) -> PermissionPrompt:
        """Display inline permission prompt widget.

        Args:
            options: Available permission options from agent.
            tool_call: The tool call requesting permission.
            result_future: Future to resolve with user's answer.
            timeout: Timeout in seconds before auto-reject.

        Returns:
            The mounted PermissionPrompt widget.
        """
        await self._remove_thinking_indicator()
        widget = PermissionPrompt(options, tool_call, result_future, timeout)
        await self.mount(widget)
        self._scroll_to_end()
        widget.focus()
        return widget

    async def post_plan_approval(self, tasks: list[Task]) -> PlanApprovalWidget:
        """Display inline plan approval widget.

        Args:
            tasks: The generated tasks to approve or dismiss.

        Returns:
            The mounted PlanApprovalWidget.
        """
        await self._remove_thinking_indicator()
        widget = PlanApprovalWidget(tasks)
        await self.mount(widget)
        self._scroll_to_end()
        widget.focus()
        return widget

    async def post_turn_separator(self) -> Rule:
        """Mount a horizontal divider between conversation turns."""
        rule = Rule(classes="turn-separator")
        await self.mount(rule)
        self._scroll_to_end()
        return rule

    def reset_turn(self) -> None:
        """Reset state for a new conversation turn.

        Note: Does NOT clear tool_calls - use clear() for full reset.
        Tool calls from previous turns remain visible in the conversation.
        """
        self._agent_response = None
        self._agent_thought = None
        self._plan_display = None
        self._xml_buffer = ""
        self._phase = "idle"

    async def clear(self) -> None:
        """Clear all content from the container."""
        await self.remove_children()
        self._agent_response = None
        self._agent_thought = None
        self._tool_calls.clear()
        self._plan_display = None
        self._thinking_indicator = None
        self._xml_buffer = ""
        self._phase = "idle"

    def _scroll_to_end(self) -> None:
        """Scroll to the bottom of the container."""
        self.scroll_end(animate=False)

    def get_text_content(self) -> str:
        """Extract all text content from the streaming output.

        Returns:
            Combined text content from all child widgets.
        """
        parts: list[str] = []

        for child in self.children:
            if isinstance(child, StreamingMarkdown):
                # StreamingMarkdown widgets - get content property
                parts.append(child.content)
            elif isinstance(child, UserInput):
                # User input has stored content
                parts.append(f"> {child._content}")
            elif isinstance(child, ToolCall):
                # Tool calls have title/status
                title = child._tool_call.title
                parts.append(f"[Tool: {title}]")
            elif isinstance(child, PlanDisplay):
                # Plan display - extract entries
                entries = [f"- {e.content}" for e in child._entries]
                if entries:
                    parts.append("Plan:\n" + "\n".join(entries))
            elif isinstance(child, Static) and not isinstance(child, ThinkingIndicator):
                # Static notes - get rendered text content
                text = str(child.render())
                if text:
                    parts.append(text)

        return "\n\n".join(parts)


def _coerce_plan_entries(entries: list[PlanEntry] | list[dict[str, object]]) -> list[PlanEntry]:
    normalized: list[PlanEntry] = []
    for entry in entries:
        if isinstance(entry, PlanEntry):
            normalized.append(entry)
            continue
        if not isinstance(entry, dict):
            continue
        data = dict(entry)
        data.setdefault("priority", "medium")
        status = data.get("status")
        if status == "failed":
            data["status"] = "completed"
        elif status is None or status not in ("pending", "in_progress", "completed"):
            data["status"] = "pending"
        try:
            normalized.append(PlanEntry.model_validate(data))
        except ValidationError:
            continue
    return normalized
