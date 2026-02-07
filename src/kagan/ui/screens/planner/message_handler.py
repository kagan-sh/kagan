"""Message handling logic for PlannerScreen."""

from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.acp import messages  # noqa: TC001 - used in method signatures at runtime

if TYPE_CHECKING:
    from kagan.ui.screens.planner.screen import PlannerScreen
    from kagan.ui.screens.planner.state import PlannerState
    from kagan.ui.widgets import StreamingOutput


class MessageHandler:
    """Handles ACP messages for the planner screen.

    This class encapsulates all message handling logic, keeping
    the main screen class focused on UI concerns.
    """

    def __init__(self, screen: PlannerScreen) -> None:
        self._screen = screen

    @property
    def state(self) -> PlannerState:
        """Get current state from screen."""
        return self._screen._state

    @state.setter
    def state(self, value: PlannerState) -> None:
        """Update state on screen."""
        self._screen._state = value

    def _get_output(self) -> StreamingOutput:
        """Get the streaming output widget."""
        return self._screen._get_output()

    def _show_output(self) -> None:
        """Show the output container."""
        self._screen._show_output()

    async def handle_agent_update(self, message: messages.AgentUpdate) -> None:
        """Handle streaming text from agent."""
        self._show_output()
        self.state.accumulated_response.append(message.text)
        await self._get_output().post_response(message.text)

    async def handle_thinking(self, message: messages.Thinking) -> None:
        """Handle thinking indicator from agent."""
        self._show_output()
        if not self.state.thinking_shown:
            self.state.thinking_shown = True
            await self._get_output().post_thinking_indicator()
        await self._get_output().post_thought(message.text)

    async def handle_tool_call(self, message: messages.ToolCall) -> None:
        """Handle tool call from agent."""
        self._show_output()
        tool_id = str(message.tool_call.get("id", "unknown"))
        title = str(message.tool_call.get("title", "Tool call"))
        kind = str(message.tool_call.get("kind", ""))
        await self._get_output().post_tool_call(tool_id, title, kind)

    def handle_tool_call_update(self, message: messages.ToolCallUpdate) -> None:
        """Handle tool call status update."""
        tool_id = str(message.update.get("id", "unknown"))
        status = str(message.update.get("status", ""))
        if status:
            self._get_output().update_tool_status(tool_id, status)

    async def handle_agent_ready(self, message: messages.AgentReady) -> None:
        """Handle agent ready notification."""
        self.state = self.state.with_agent_ready(True)
        self._screen._enable_input()
        self._screen._update_status("ready", "Press F1 for help")

    async def handle_agent_fail(self, message: messages.AgentFail) -> None:
        """Handle agent failure."""
        self._screen._update_status("error", f"Error: {message.message}")
        self._screen._disable_input()

        self._show_output()
        output = self._get_output()
        await output.post_note(f"Error: {message.message}", classes="error")
        if message.details:
            await output.post_note(message.details)

    async def handle_plan(self, message: messages.Plan) -> None:
        """Display plan entries from agent."""
        self._show_output()
        await self._get_output().post_plan(message.entries)

    def handle_set_modes(self, message: messages.SetModes) -> None:
        """Store available modes from agent."""
        self._screen._current_mode = message.current_mode
        self._screen._available_modes = message.modes

    def handle_mode_update(self, message: messages.ModeUpdate) -> None:
        """Track mode changes from agent."""
        self._screen._current_mode = message.current_mode

    def handle_commands_update(self, message: messages.AvailableCommandsUpdate) -> None:
        """Store available slash commands from agent."""
        self._screen._available_commands = message.commands

    async def handle_request_permission(self, message: messages.RequestPermission) -> None:
        """Display inline permission prompt when agent requests it."""
        self._show_output()
        await self._get_output().post_permission_request(
            message.options,
            message.tool_call,
            message.result_future,
            timeout=300.0,
        )
