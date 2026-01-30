"""Streaming output container for agent conversation display."""

from __future__ import annotations

import re
from typing import TYPE_CHECKING, Literal
from uuid import uuid4

from textual.containers import VerticalScroll
from textual.widgets import Rule, Static

from kagan.ui.widgets.agent_content import AgentResponse, AgentThought, UserInput
from kagan.ui.widgets.permission_prompt import PermissionPrompt
from kagan.ui.widgets.plan_display import PlanDisplay
from kagan.ui.widgets.tool_call import ToolCall

if TYPE_CHECKING:
    import asyncio

    from textual.app import ComposeResult
    from textual.widget import Widget

    from kagan.acp import protocol
    from kagan.acp.messages import Answer

# Phase state machine
StreamPhase = Literal["idle", "thinking", "streaming", "complete"]

# Regex to strip plan/todos XML blocks from response text
XML_BLOCK_PATTERN = re.compile(r"<(todos|plan)>.*?</\1>", re.DOTALL | re.IGNORECASE)


class ThinkingIndicator(Static):
    """Simple thinking indicator widget."""

    def __init__(self, **kwargs) -> None:
        super().__init__("Thinking...", **kwargs)


class StreamingOutput(VerticalScroll):
    """Container for streaming agent conversation content."""

    def __init__(self, *, id: str | None = None, classes: str | None = None) -> None:
        super().__init__(id=id, classes=classes)
        self._agent_response: AgentResponse | None = None
        self._agent_thought: AgentThought | None = None
        self._tool_calls: dict[str, ToolCall] = {}
        self._plan_display: PlanDisplay | None = None
        self._thinking_indicator: ThinkingIndicator | None = None
        self._phase: StreamPhase = "idle"

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

    async def post_response(self, fragment: str = "") -> AgentResponse:
        """Get or create agent response widget. Resets thought state."""
        await self._remove_thinking_indicator()
        self._agent_thought = None
        self._phase = "streaming"

        # Filter out XML blocks from fragment
        if fragment:
            fragment = XML_BLOCK_PATTERN.sub("", fragment)

        if self._agent_response is None:
            self._agent_response = AgentResponse(fragment)
            await self.mount(self._agent_response)
        elif fragment:
            await self._agent_response.append_fragment(fragment)
        self._scroll_to_end()
        return self._agent_response

    async def post_thought(self, fragment: str) -> AgentThought:
        """Get or create agent thought widget."""
        await self._remove_thinking_indicator()
        if self._agent_thought is None:
            self._agent_thought = AgentThought(fragment)
            await self.mount(self._agent_thought)
        else:
            await self._agent_thought.append_fragment(fragment)
        self._scroll_to_end()
        return self._agent_thought

    async def post_tool_call(self, tool_id: str, title: str, kind: str = "") -> ToolCall:
        """Post a tool call notification."""
        await self._remove_thinking_indicator()
        self._agent_response = None
        self._agent_thought = None

        # Generate unique ID if tool_id is unknown/empty
        if not tool_id or tool_id == "unknown":
            tool_id = f"auto-{uuid4().hex[:8]}"

        tool_data = {"id": tool_id, "title": title, "kind": kind, "status": "pending"}
        widget = ToolCall(tool_data, id=f"tool-{tool_id}")
        self._tool_calls[tool_id] = widget
        await self.mount(widget)
        self._scroll_to_end()
        return widget

    def update_tool_status(self, tool_id: str, status: str) -> None:
        """Update a tool call's status."""
        if tool_id in self._tool_calls:
            self._tool_calls[tool_id].update_status(status)

    async def post_note(self, text: str, classes: str = "") -> Widget:
        """Post a simple text note."""
        widget = Static(text, classes=f"streaming-note {classes}".strip())
        await self.mount(widget)
        self._scroll_to_end()
        return widget

    async def post_plan(self, entries: list[protocol.PlanEntry]) -> PlanDisplay:
        """Display agent plan entries. Updates existing if present."""
        self._agent_thought = None
        # Do NOT reset _agent_response here - causes line overwriting bug

        if self._plan_display is not None:
            # Update existing plan display in-place
            self._plan_display.update_entries(entries)
        else:
            self._plan_display = PlanDisplay(entries, classes="plan-display")
            await self.mount(self._plan_display)

        self._scroll_to_end()
        return self._plan_display

    async def post_permission_request(
        self,
        options: list[protocol.PermissionOption],
        tool_call: protocol.ToolCall | protocol.ToolCallUpdatePermissionRequest,
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

    async def post_turn_separator(self) -> Rule:
        """Mount a horizontal divider between conversation turns."""
        rule = Rule(classes="turn-separator")
        await self.mount(rule)
        self._scroll_to_end()
        return rule

    def reset_turn(self) -> None:
        """Reset state for a new conversation turn."""
        self._agent_response = None
        self._agent_thought = None
        self._plan_display = None
        self._phase = "idle"

    async def clear(self) -> None:
        """Clear all content from the container."""
        await self.remove_children()
        self._agent_response = None
        self._agent_thought = None
        self._tool_calls.clear()
        self._plan_display = None
        self._thinking_indicator = None
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
            if isinstance(child, (AgentResponse, AgentThought)):
                # Markdown widgets - get the markdown source
                if child._markdown:
                    parts.append(child._markdown)
            elif isinstance(child, UserInput):
                # User input has stored content
                parts.append(f"> {child._content}")
            elif isinstance(child, ToolCall):
                # Tool calls have title/status
                title = child._tool_call.get("title", "Tool Call")
                parts.append(f"[Tool: {title}]")
            elif isinstance(child, PlanDisplay):
                # Plan display - extract entries
                entries = [f"- {e.get('content', '')}" for e in child._entries]
                if entries:
                    parts.append("Plan:\n" + "\n".join(entries))
            elif isinstance(child, Static) and not isinstance(child, ThinkingIndicator):
                # Static notes - get rendered text content
                text = str(child.render())
                if text:
                    parts.append(text)

        return "\n\n".join(parts)
