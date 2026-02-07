from __future__ import annotations

from typing import TYPE_CHECKING, cast
from unittest.mock import AsyncMock

import pytest
from acp.schema import ToolCall as AcpToolCall
from acp.schema import ToolCallUpdate as AcpToolCallUpdate

from kagan.acp import messages
from kagan.ui.screens.planner.message_handler import MessageHandler
from kagan.ui.screens.planner.state import PlannerState

if TYPE_CHECKING:
    from kagan.ui.screens.planner.screen import PlannerScreen


class _OutputStub:
    def __init__(self) -> None:
        self.upsert_tool_call = AsyncMock()
        self.apply_tool_call_update = AsyncMock()


class _ScreenStub:
    def __init__(self, output: _OutputStub) -> None:
        self._state = PlannerState()
        self._output = output
        self.output_shown = False

    def _get_output(self) -> _OutputStub:
        return self._output

    def _show_output(self) -> None:
        self.output_shown = True


@pytest.mark.asyncio
async def test_planner_tool_call_uses_full_payload_upsert() -> None:
    output = _OutputStub()
    screen = _ScreenStub(output)
    handler = MessageHandler(cast("PlannerScreen", screen))

    tool_call = AcpToolCall(
        toolCallId="tc-read-1",
        title="ReadFile",
        kind="read",
        rawInput={"path": "docs/index.md"},
    )

    await handler.handle_tool_call(messages.ToolCall(tool_call))

    assert screen.output_shown is True
    output.upsert_tool_call.assert_awaited_once_with(tool_call)


@pytest.mark.asyncio
async def test_planner_tool_call_update_applies_full_tool_record() -> None:
    output = _OutputStub()
    screen = _ScreenStub(output)
    handler = MessageHandler(cast("PlannerScreen", screen))

    tool_call = AcpToolCall(
        toolCallId="tc-read-1",
        title="ReadFile",
        kind="read",
        rawInput={"path": "docs/index.md"},
    )
    update = AcpToolCallUpdate(
        toolCallId="tc-read-1",
        status="completed",
    )

    await handler.handle_tool_call_update(messages.ToolCallUpdate(tool_call, update))

    assert screen.output_shown is True
    output.apply_tool_call_update.assert_awaited_once_with(update, tool_call)
