from __future__ import annotations

import json
from types import SimpleNamespace
from typing import Any, cast

from acp.schema import ToolCall as AcpToolCall
from acp.schema import ToolCallUpdate as AcpToolCallUpdate

from kagan.acp import messages
from kagan.agents.output import serialize_agent_output


class _FakeAgent:
    def __init__(self, buffered_messages: list[object], response_text: str) -> None:
        self._buffers = SimpleNamespace(messages=buffered_messages)
        self._response_text = response_text

    def get_response_text(self) -> str:
        return self._response_text


def test_serialize_agent_output_compacts_streamed_text_and_drops_thinking() -> None:
    agent = _FakeAgent(
        buffered_messages=[
            messages.AgentUpdate("text", "Hello"),
            messages.AgentUpdate("text", " world"),
            messages.Thinking("text", "private reasoning"),
            messages.AgentUpdate("terminal", "$ ls -la"),
        ],
        response_text="Hello world",
    )

    payload = json.loads(serialize_agent_output(cast("Any", agent)))

    assert payload == {
        "messages": [
            {"type": "response", "content": "$ ls -la"},
            {"type": "response", "content": "Hello world"},
        ]
    }
    assert "response_text" not in payload


def test_serialize_agent_output_can_include_thinking() -> None:
    agent = _FakeAgent(
        buffered_messages=[messages.Thinking("text", "short thought")],
        response_text="Done",
    )

    payload = json.loads(serialize_agent_output(cast("Any", agent), include_thinking=True))

    assert payload == {
        "messages": [
            {"type": "thinking", "content": "short thought"},
            {"type": "response", "content": "Done"},
        ]
    }


def test_serialize_agent_output_keeps_tool_call_and_fail_events() -> None:
    tool_call = AcpToolCall(
        toolCallId="tc-1",
        title="Write file",
        kind="edit",
        status="completed",
        content=[
            {
                "type": "content",
                "content": {
                    "type": "text",
                    "text": "updated file",
                },
            }
        ],
        rawInput='{"path":"app.py"}',
        rawOutput='{"ok":true}',
    )
    agent = _FakeAgent(
        buffered_messages=[
            messages.ToolCall(tool_call),
            messages.AgentFail("Failed", "Tool error"),
        ],
        response_text="",
    )

    payload = json.loads(serialize_agent_output(cast("Any", agent)))

    assert payload == {
        "messages": [
            {
                "type": "tool_call",
                "id": "tc-1",
                "title": "Write file",
                "kind": "edit",
                "status": "completed",
                "content": [
                    {
                        "type": "content",
                        "content": {
                            "type": "text",
                            "text": "updated file",
                        },
                    }
                ],
                "raw_input": '{"path":"app.py"}',
                "raw_output": '{"ok":true}',
            },
            {"type": "agent_fail", "message": "Failed", "details": "Tool error"},
        ]
    }


def test_serialize_agent_output_includes_tool_call_update_payload() -> None:
    tool_call = AcpToolCall(
        toolCallId="tc-2",
        title="Read file",
        kind="read",
        status="completed",
        content=[
            {
                "type": "content",
                "content": {
                    "type": "text",
                    "text": "hello",
                },
            }
        ],
    )
    update = AcpToolCallUpdate(toolCallId="tc-2", status="completed")

    agent = _FakeAgent(
        buffered_messages=[messages.ToolCallUpdate(tool_call, update)],
        response_text="",
    )

    payload = json.loads(serialize_agent_output(cast("Any", agent)))
    assert payload == {
        "messages": [
            {
                "type": "tool_call_update",
                "id": "tc-2",
                "status": "completed",
                "title": "Read file",
                "kind": "read",
                "content": [
                    {
                        "type": "content",
                        "content": {
                            "type": "text",
                            "text": "hello",
                        },
                    }
                ],
                "raw_input": None,
                "raw_output": None,
            }
        ]
    }
