"""Core mock agent implementation for test flows."""

from __future__ import annotations

import re
from typing import TYPE_CHECKING, Any

from acp.schema import ToolCall

from kagan.core.acp.messages import AgentBuffers

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.core.config import AgentConfig


def _coerce_tool_call(tool_call: Any) -> ToolCall:
    if isinstance(tool_call, ToolCall):
        return tool_call
    if not isinstance(tool_call, dict):
        return ToolCall(toolCallId="unknown", title="Tool call")
    data = dict(tool_call)
    if "toolCallId" not in data:
        if "id" in data:
            data["toolCallId"] = data["id"]
        elif "tool_call_id" in data:
            data["toolCallId"] = data["tool_call_id"]
    if not data.get("title"):
        data["title"] = data.get("name") or "Tool call"
    if "rawInput" not in data:
        for key in ("arguments", "input", "params", "args"):
            if key in data:
                data["rawInput"] = data[key]
                break
    return ToolCall.model_validate(data)


class MockAgent:
    """Mock ACP agent with controllable responses for snapshot / E2E testing.

    Simulates the Agent interface without spawning real processes.
    Responses are controlled via ``set_response`` / ``set_tool_calls``.
    """

    def __init__(
        self,
        project_root: Path,
        agent_config: AgentConfig,
        *,
        read_only: bool = False,
    ) -> None:
        self.project_root = project_root
        self._agent_config = agent_config
        self._read_only = read_only
        self._buffers = AgentBuffers()
        self._tool_calls: dict[str, ToolCall] = {}
        self._thinking_text: str = ""
        self._ready = False
        self._stopped = False
        self._auto_approve = False
        self._model_override: str | None = None
        self._message_target: Any = None

        self._buffers.append_response("Done. <complete/>")

    def _stream_text(self) -> str:
        text = self._buffers.get_response_text()
        if not text:
            return ""
        text = re.sub(r"<complete\\s*/?>", "", text, flags=re.IGNORECASE)
        text = re.sub(r"<continue\\s*/?>", "", text, flags=re.IGNORECASE)
        text = re.sub(r"<blocked\\s+reason=\"[^\"]+\"\\s*/?>", "", text, flags=re.IGNORECASE)
        text = re.sub(r"<approve\\s*[^>]*?/?>", "", text, flags=re.IGNORECASE)
        text = re.sub(r"<reject\\s+reason=\"[^\"]+\"\\s*/?>", "", text, flags=re.IGNORECASE)
        return text.strip()

    def set_response(self, text: str) -> None:
        self._buffers.clear_response()
        self._buffers.append_response(text)

    def set_tool_calls(self, tool_calls: dict[str, Any]) -> None:
        self._tool_calls = {
            tool_call_id: _coerce_tool_call(tool_call)
            for tool_call_id, tool_call in tool_calls.items()
        }

    def set_thinking_text(self, text: str) -> None:
        self._thinking_text = text

    def set_message_target(self, target: Any) -> None:
        self._message_target = target

    def set_auto_approve(self, enabled: bool) -> None:
        self._auto_approve = enabled

    def set_model_override(self, model_id: str | None) -> None:
        self._model_override = model_id

    def start(self, message_target: Any = None) -> None:
        self._message_target = message_target
        self._ready = True
        if self._message_target:
            from kagan.core.acp import messages

            self._message_target.post_message(messages.AgentReady())

    async def wait_ready(self, timeout: float = 30.0) -> None:
        self._ready = True
        if self._message_target:
            from kagan.core.acp import messages

            self._message_target.post_message(messages.AgentReady())

    async def send_prompt(self, prompt: str) -> str | None:
        import asyncio

        if self._message_target:
            from kagan.core.acp import messages

            if self._thinking_text:
                self._message_target.post_message(messages.Thinking("text", self._thinking_text))
                await asyncio.sleep(0.05)

            stream_text = self._stream_text()
            if stream_text:
                self._message_target.post_message(messages.AgentUpdate("text", stream_text))
                await asyncio.sleep(0.1)

            for tool_call in self._tool_calls.values():
                self._message_target.post_message(messages.ToolCall(tool_call))
                await asyncio.sleep(0.05)

            await asyncio.sleep(0.05)
            self._message_target.post_message(messages.AgentComplete())
            await asyncio.sleep(0.05)

        return "end_turn"

    def get_response_text(self) -> str:
        return self._buffers.get_response_text()

    def get_messages(self) -> list[Any]:
        return list(self._buffers.messages)

    def get_tool_calls(self) -> dict[str, ToolCall]:
        return self._tool_calls

    @property
    def tool_calls(self) -> dict[str, ToolCall]:
        return self._tool_calls

    def get_thinking_text(self) -> str:
        return self._thinking_text

    def clear_tool_calls(self) -> None:
        self._tool_calls.clear()

    async def stop(self) -> None:
        self._stopped = True
        self._buffers.clear_all()

    async def cancel(self) -> bool:
        return True
