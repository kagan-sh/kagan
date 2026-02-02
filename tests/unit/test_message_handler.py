"""Unit tests for MessageHandler.

Tests the ACP message handling logic extracted from PlannerScreen.
"""

from __future__ import annotations

from dataclasses import replace
from typing import Any
from unittest.mock import AsyncMock, MagicMock

import pytest

from kagan.acp import messages
from kagan.ui.screens.planner.message_handler import MessageHandler
from kagan.ui.screens.planner.state import PlannerPhase, PlannerState

pytestmark = pytest.mark.unit


@pytest.fixture
def mock_screen() -> MagicMock:
    """Create a mock PlannerScreen with required attributes."""
    screen = MagicMock()
    screen._state = PlannerState()
    screen._get_output = MagicMock()
    screen._show_output = MagicMock()
    screen._enable_input = MagicMock()
    screen._disable_input = MagicMock()
    screen._update_status = MagicMock()
    screen._current_mode = None
    screen._available_modes = []
    screen._available_commands = []
    return screen


@pytest.fixture
def message_handler(mock_screen: MagicMock) -> MessageHandler:
    """Create a MessageHandler with mocked screen."""
    return MessageHandler(mock_screen)


@pytest.fixture
def mock_output() -> MagicMock:
    """Create a mock StreamingOutput widget."""
    output = MagicMock()
    output.post_response = AsyncMock()
    output.post_thinking_indicator = AsyncMock()
    output.post_thought = AsyncMock()
    output.post_tool_call = AsyncMock()
    output.update_tool_status = MagicMock()
    output.post_plan = AsyncMock()
    output.post_note = AsyncMock()
    output.post_permission_request = AsyncMock()
    output._plan_display = None
    return output


# =============================================================================
# State Property Tests
# =============================================================================


class TestStateProperty:
    """Tests for state getter and setter."""

    def test_state_getter_returns_screen_state(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
    ) -> None:
        """state property returns screen._state."""
        state = PlannerState(phase=PlannerPhase.AWAITING_APPROVAL)
        mock_screen._state = state

        assert message_handler.state is state

    def test_state_setter_updates_screen_state(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
    ) -> None:
        """state setter updates screen._state."""
        new_state = PlannerState(phase=PlannerPhase.PROCESSING)
        message_handler.state = new_state

        assert mock_screen._state is new_state


# =============================================================================
# handle_agent_update Tests
# =============================================================================


class TestHandleAgentUpdate:
    """Tests for handling AgentUpdate messages."""

    async def test_shows_output(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Shows output container when receiving update."""
        mock_screen._get_output.return_value = mock_output
        msg = messages.AgentUpdate(content_type="text", text="Hello")

        await message_handler.handle_agent_update(msg)

        mock_screen._show_output.assert_called_once()

    async def test_accumulates_response(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Accumulates response text in state."""
        mock_screen._get_output.return_value = mock_output
        msg1 = messages.AgentUpdate(content_type="text", text="Hello ")
        msg2 = messages.AgentUpdate(content_type="text", text="World")

        await message_handler.handle_agent_update(msg1)
        await message_handler.handle_agent_update(msg2)

        assert message_handler.state.accumulated_response == ["Hello ", "World"]

    async def test_posts_response_to_output(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Posts response text to output widget."""
        mock_screen._get_output.return_value = mock_output
        msg = messages.AgentUpdate(content_type="text", text="Test message")

        await message_handler.handle_agent_update(msg)

        mock_output.post_response.assert_called_once_with("Test message")


# =============================================================================
# handle_thinking Tests
# =============================================================================


class TestHandleThinking:
    """Tests for handling Thinking messages."""

    async def test_shows_indicator_on_first_thinking(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Shows thinking indicator on first thinking message."""
        mock_screen._get_output.return_value = mock_output
        msg = messages.Thinking(content_type="thinking", text="Thinking...")

        await message_handler.handle_thinking(msg)

        mock_output.post_thinking_indicator.assert_called_once()

    async def test_does_not_show_indicator_twice(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Does not show thinking indicator twice."""
        mock_screen._get_output.return_value = mock_output
        # Set thinking_shown to True using dataclass replace
        mock_screen._state = replace(mock_screen._state, thinking_shown=True)
        msg = messages.Thinking(content_type="thinking", text="More thinking...")

        await message_handler.handle_thinking(msg)

        mock_output.post_thinking_indicator.assert_not_called()

    async def test_posts_thought_to_output(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Posts thought text to output widget."""
        mock_screen._get_output.return_value = mock_output
        msg = messages.Thinking(content_type="thinking", text="Deep thoughts...")

        await message_handler.handle_thinking(msg)

        mock_output.post_thought.assert_called_once_with("Deep thoughts...")


# =============================================================================
# handle_tool_call Tests
# =============================================================================


class TestHandleToolCall:
    """Tests for handling ToolCall messages."""

    async def test_extracts_tool_call_fields(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Extracts id, title, kind from tool_call dict."""
        mock_screen._get_output.return_value = mock_output
        msg = messages.ToolCall(
            tool_call={
                "id": "tool-123",
                "title": "Reading file",
                "kind": "read",
            }
        )

        await message_handler.handle_tool_call(msg)

        mock_output.post_tool_call.assert_called_once_with("tool-123", "Reading file", "read")

    async def test_uses_defaults_for_missing_fields(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Uses default values for missing fields."""
        mock_screen._get_output.return_value = mock_output
        msg = messages.ToolCall(tool_call={})

        await message_handler.handle_tool_call(msg)

        mock_output.post_tool_call.assert_called_once_with("unknown", "Tool call", "")


# =============================================================================
# handle_tool_call_update Tests
# =============================================================================


class TestHandleToolCallUpdate:
    """Tests for handling ToolCallUpdate messages."""

    def test_updates_tool_status(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Updates tool status in output widget."""
        mock_screen._get_output.return_value = mock_output
        msg = messages.ToolCallUpdate(
            tool_call={"id": "tool-123"},
            update={"id": "tool-123", "status": "completed"},
        )

        message_handler.handle_tool_call_update(msg)

        mock_output.update_tool_status.assert_called_once_with("tool-123", "completed")

    def test_ignores_empty_status(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Does not update if status is empty."""
        mock_screen._get_output.return_value = mock_output
        msg = messages.ToolCallUpdate(
            tool_call={"id": "tool-123"},
            update={"id": "tool-123", "status": ""},
        )

        message_handler.handle_tool_call_update(msg)

        mock_output.update_tool_status.assert_not_called()


# =============================================================================
# handle_agent_ready Tests
# =============================================================================


class TestHandleAgentReady:
    """Tests for handling AgentReady messages."""

    async def test_updates_state_with_agent_ready(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
    ) -> None:
        """Sets agent_ready=True in state."""
        msg = messages.AgentReady()

        await message_handler.handle_agent_ready(msg)

        assert mock_screen._state.agent_ready is True

    async def test_enables_input(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
    ) -> None:
        """Enables input when agent is ready."""
        msg = messages.AgentReady()

        await message_handler.handle_agent_ready(msg)

        mock_screen._enable_input.assert_called_once()

    async def test_updates_status_to_ready(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
    ) -> None:
        """Updates status bar to show ready."""
        msg = messages.AgentReady()

        await message_handler.handle_agent_ready(msg)

        mock_screen._update_status.assert_called_once_with("ready", "Press F1 for help")


# =============================================================================
# handle_agent_fail Tests
# =============================================================================


class TestHandleAgentFail:
    """Tests for handling AgentFail messages."""

    async def test_updates_status_with_error(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Updates status bar with error message."""
        mock_screen._get_output.return_value = mock_output
        msg = messages.AgentFail(message="Connection lost")

        await message_handler.handle_agent_fail(msg)

        mock_screen._update_status.assert_called_once_with("error", "Error: Connection lost")

    async def test_disables_input(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Disables input when agent fails."""
        mock_screen._get_output.return_value = mock_output
        msg = messages.AgentFail(message="Error")

        await message_handler.handle_agent_fail(msg)

        mock_screen._disable_input.assert_called_once()

    async def test_posts_error_to_output(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Posts error message to output."""
        mock_screen._get_output.return_value = mock_output
        msg = messages.AgentFail(message="Timeout", details="Connection timed out after 30s")

        await message_handler.handle_agent_fail(msg)

        mock_output.post_note.assert_any_call("Error: Timeout", classes="error")
        mock_output.post_note.assert_any_call("Connection timed out after 30s")


# =============================================================================
# handle_plan Tests
# =============================================================================


class TestHandlePlan:
    """Tests for handling Plan messages."""

    async def test_shows_output(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Shows output container."""
        mock_screen._get_output.return_value = mock_output
        msg = messages.Plan(entries=[{"title": "Task 1"}])

        await message_handler.handle_plan(msg)

        mock_screen._show_output.assert_called_once()

    async def test_posts_plan_entries(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Posts plan entries to output."""
        mock_screen._get_output.return_value = mock_output
        entries = [{"title": "Task 1"}, {"title": "Task 2"}]
        msg = messages.Plan(entries=entries)

        await message_handler.handle_plan(msg)

        mock_output.post_plan.assert_called_once_with(entries)


# =============================================================================
# handle_set_modes / handle_mode_update Tests
# =============================================================================


class TestModeHandling:
    """Tests for mode-related message handling."""

    def test_set_modes_stores_modes(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
    ) -> None:
        """Stores available modes from SetModes message."""
        modes = {
            "code": messages.Mode("code", "Code", None),
            "plan": messages.Mode("plan", "Plan", None),
            "chat": messages.Mode("chat", "Chat", None),
        }
        msg = messages.SetModes(modes=modes, current_mode="code")

        message_handler.handle_set_modes(msg)

        assert mock_screen._current_mode == "code"
        assert mock_screen._available_modes == modes

    def test_mode_update_tracks_current(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
    ) -> None:
        """Tracks current mode from ModeUpdate message."""
        msg = messages.ModeUpdate(current_mode="plan")

        message_handler.handle_mode_update(msg)

        assert mock_screen._current_mode == "plan"


# =============================================================================
# handle_commands_update Tests
# =============================================================================


class TestHandleCommandsUpdate:
    """Tests for handling AvailableCommandsUpdate messages."""

    def test_stores_available_commands(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
    ) -> None:
        """Stores available commands from message."""
        commands: list[dict[str, Any]] = [
            {"name": "/help"},
            {"name": "/clear"},
            {"name": "/mode"},
        ]
        msg = messages.AvailableCommandsUpdate(commands=commands)

        message_handler.handle_commands_update(msg)

        assert mock_screen._available_commands == commands


# =============================================================================
# handle_request_permission Tests
# =============================================================================


class TestHandleRequestPermission:
    """Tests for handling RequestPermission messages."""

    async def test_shows_output(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Shows output container."""
        mock_screen._get_output.return_value = mock_output
        import asyncio

        future = asyncio.get_event_loop().create_future()
        options: list[dict[str, Any]] = [{"label": "Allow"}, {"label": "Deny"}]
        msg = messages.RequestPermission(
            options=options,
            tool_call={"id": "tool-1"},
            result_future=future,
        )

        await message_handler.handle_request_permission(msg)

        mock_screen._show_output.assert_called_once()

    async def test_posts_permission_request(
        self,
        message_handler: MessageHandler,
        mock_screen: MagicMock,
        mock_output: MagicMock,
    ) -> None:
        """Posts permission request with correct parameters."""
        mock_screen._get_output.return_value = mock_output
        import asyncio

        future = asyncio.get_event_loop().create_future()
        options: list[dict[str, Any]] = [
            {"label": "Allow"},
            {"label": "Deny"},
            {"label": "Always Allow"},
        ]
        tool_call = {"id": "tool-1", "name": "write_file"}
        msg = messages.RequestPermission(
            options=options,
            tool_call=tool_call,
            result_future=future,
        )

        await message_handler.handle_request_permission(msg)

        mock_output.post_permission_request.assert_called_once_with(
            options,
            tool_call,
            future,
            timeout=300.0,
        )
