"""Chat panel combining streaming output with optional follow-up input."""

from __future__ import annotations

import contextlib
import json
from dataclasses import dataclass
from typing import TYPE_CHECKING, ClassVar, cast

from acp.schema import ToolCall as AcpToolCall
from textual import events, on
from textual.binding import Binding, BindingType
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.message import Message
from textual.widgets import Button, Label, Static, TextArea

from kagan.core.models.enums import MessageType
from kagan.ui.widgets.streaming_output import StreamingOutput

if TYPE_CHECKING:
    from collections.abc import Awaitable, Callable

    from textual.app import ComposeResult

    from kagan.app import KaganApp
    from kagan.services.queued_messages import QueuedMessage


class ChatInput(TextArea):
    """Text input that submits on Enter (Shift+Enter/Ctrl+J for newline)."""

    BINDINGS: ClassVar[list[BindingType]] = [
        Binding("shift+enter,ctrl+j", "insert_newline", "New Line", show=False, priority=True),
    ]

    @dataclass
    class SubmitRequested(Message):
        text: str

    def __init__(
        self,
        text: str = "",
        *,
        placeholder: str | None = None,
        **kwargs,
    ) -> None:
        super().__init__(text, **kwargs)
        self._placeholder = placeholder

    def on_mount(self) -> None:
        if self._placeholder:
            with contextlib.suppress(Exception):
                self.placeholder = self._placeholder

    def action_insert_newline(self) -> None:
        """Insert a newline character."""
        self.insert("\n")

    async def _on_key(self, event: events.Key) -> None:
        if event.key == "enter":
            event.prevent_default()
            event.stop()
            self.post_message(self.SubmitRequested(self.text))
            return
        await super()._on_key(event)


class QueuedMessageRow(Horizontal):
    """A single queued message with remove button."""

    @dataclass
    class RemoveRequested(Message):
        index: int

    def __init__(self, content: str, index: int, **kwargs) -> None:
        super().__init__(**kwargs)
        self._content = content
        self._index = index

    def compose(self) -> ComposeResult:
        # Truncate long messages for display
        display_text = self._content
        if len(display_text) > 80:
            display_text = display_text[:77] + "..."
        yield Label(display_text, classes="queued-message-text")
        yield Button("✗", classes="queued-message-remove", id=f"remove-{self._index}")

    @on(Button.Pressed)
    def _on_remove_pressed(self) -> None:
        self.post_message(self.RemoveRequested(self._index))


class QueuedMessagesContainer(VerticalScroll):
    """Container for displaying queued messages."""

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self._messages: list[QueuedMessage] = []

    def compose(self) -> ComposeResult:
        yield Static("Queued messages", classes="queued-messages-header")

    def update_messages(self, messages: list[QueuedMessage]) -> None:
        """Update the displayed queued messages."""
        self._messages = messages
        # Remove existing message rows
        for row in self.query(QueuedMessageRow):
            row.remove()
        # Add new rows
        for idx, msg in enumerate(messages):
            self.mount(QueuedMessageRow(msg.content, idx, classes="queued-message-row"))
        # Show/hide based on whether there are messages
        self.display = len(messages) > 0


class ChatPanel(Vertical):
    """Chat UI with streaming output and optional follow-up input."""

    DEFAULT_CLASSES = "chat-panel"

    def __init__(
        self,
        execution_id: str | None,
        *,
        allow_input: bool,
        input_placeholder: str | None = None,
        output_id: str | None = None,
        id: str | None = None,
        classes: str | None = None,
    ) -> None:
        super().__init__(id=id, classes=classes)
        self._execution_id = execution_id
        self._allow_input = allow_input
        self._input_placeholder = input_placeholder or "Press Enter to queue message for next run"
        self._output_id = output_id
        self._send_handler: Callable[[str], Awaitable[None]] | None = None
        self._remove_handler: Callable[[int], Awaitable[bool]] | None = None
        self._get_queued_handler: Callable[[], Awaitable[list[QueuedMessage]]] | None = None

    def compose(self) -> ComposeResult:
        yield StreamingOutput(id=self._output_id, classes="chat-output")
        if self._allow_input:
            with Vertical(classes="chat-input"):
                yield QueuedMessagesContainer(classes="queued-messages-container")
                yield ChatInput(
                    "",
                    placeholder=self._input_placeholder,
                    show_line_numbers=False,
                    classes="chat-input-area",
                )

    def on_mount(self) -> None:
        if self._allow_input:
            # Hide queued messages container initially
            container = self.query_one(QueuedMessagesContainer)
            container.display = False

    @property
    def output(self) -> StreamingOutput:
        return self.query_one(StreamingOutput)

    def set_execution_id(self, execution_id: str | None) -> None:
        self._execution_id = execution_id

    async def load_logs(self) -> None:
        if not self._execution_id:
            return
        app = cast("KaganApp", self.app)
        logs = await app.ctx.execution_service.get_logs(self._execution_id)
        if logs is None or not logs.logs:
            return
        for line in logs.logs.splitlines():
            await self._render_log_line(line)

    async def append_local_message(self, content: str, author: str = "You") -> None:
        if not content.strip():
            return
        if author.strip().lower() in ("you", "user"):
            await self.output.post_user_input(content)
        else:
            await self.output.post_note(f"{author}: {content}", classes="info")

    def get_text_content(self) -> str:
        return self.output.get_text_content()

    def set_send_handler(self, handler: Callable[[str], Awaitable[None]] | None) -> None:
        self._send_handler = handler

    def set_remove_handler(self, handler: Callable[[int], Awaitable[bool]] | None) -> None:
        self._remove_handler = handler

    def set_get_queued_handler(
        self, handler: Callable[[], Awaitable[list[QueuedMessage]]] | None
    ) -> None:
        self._get_queued_handler = handler

    async def refresh_queued_messages(self) -> None:
        """Refresh the queued messages display."""
        if not self._allow_input or not self._get_queued_handler:
            return
        messages = await self._get_queued_handler()
        container = self.query_one(QueuedMessagesContainer)
        container.update_messages(messages)

    def _get_input(self) -> ChatInput:
        return self.query_one(ChatInput)

    @on(QueuedMessageRow.RemoveRequested)
    async def _on_remove_requested(self, message: QueuedMessageRow.RemoveRequested) -> None:
        if self._remove_handler:
            await self._remove_handler(message.index)
            await self.refresh_queued_messages()

    @on(ChatInput.SubmitRequested)
    async def _on_input_submit(self, message: ChatInput.SubmitRequested) -> None:
        await self._submit_message(message.text)

    async def _submit_message(self, text: str) -> None:
        cleaned = text.strip()
        if not cleaned:
            return
        if self._send_handler is None:
            self.notify("Follow-up messaging unavailable", severity="warning")
            return
        try:
            await self._send_handler(cleaned)
        except Exception as exc:  # pragma: no cover - UI safety net
            self.notify(f"Send failed: {exc}", severity="error")
            return
        self._get_input().text = ""
        self._get_input().focus()
        # Refresh queued messages display
        await self.refresh_queued_messages()

    async def _render_log_line(self, log_line: str) -> None:
        """Render a single JSONL log line into the output."""
        try:
            data = json.loads(log_line)
        except json.JSONDecodeError:
            await self.output.post_note(
                "Unsupported log format (expected JSON).", classes="warning"
            )
            return

        message_entries = data.get("messages", [])
        for msg in message_entries:
            msg_type = msg.get("type", "")
            if msg_type == MessageType.RESPONSE:
                content = msg.get("content", "")
                if content:
                    await self.output.post_response(content)
            elif msg_type == MessageType.THINKING:
                content = msg.get("content", "")
                if content:
                    await self.output.post_thought(content)
            elif msg_type == MessageType.TOOL_CALL or msg_type == MessageType.TOOL_CALL_UPDATE:
                tool_call = AcpToolCall.model_validate(
                    {
                        "toolCallId": msg.get("id", "unknown"),
                        "title": msg.get("title", "Tool call"),
                        "kind": msg.get("kind") or None,
                        "status": msg.get("status") or None,
                        "content": msg.get("content"),
                        "rawInput": msg.get("raw_input"),
                        "rawOutput": msg.get("raw_output"),
                    }
                )
                await self.output.upsert_tool_call(tool_call)
            elif msg_type == MessageType.PLAN:
                plan_entries = msg.get("entries", [])
                if plan_entries:
                    await self.output.post_plan(plan_entries)
            elif msg_type == MessageType.AGENT_READY:
                await self.output.post_note("Agent ready", classes="success")
            elif msg_type == MessageType.AGENT_FAIL:
                error_msg = msg.get("message", "Unknown error")
                await self.output.post_note(f"Error: {error_msg}", classes="error")
                details = msg.get("details")
                if details:
                    await self.output.post_note(details)

        if not message_entries:
            response_text = data.get("response_text", "")
            if response_text:
                await self.output.post_response(response_text)


__all__ = ["ChatPanel"]
