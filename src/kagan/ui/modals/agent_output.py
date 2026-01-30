"""Modal for watching AUTO ticket agent progress."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

from textual import on
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Label, Rule

from kagan.acp import messages
from kagan.acp.messages import Answer
from kagan.constants import MODAL_TITLE_MAX_LENGTH
from kagan.keybindings import AGENT_OUTPUT_BINDINGS, to_textual_bindings
from kagan.ui.utils.clipboard import copy_with_notification
from kagan.ui.widgets import StreamingOutput

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.acp import protocol
    from kagan.acp.agent import Agent
    from kagan.app import KaganApp
    from kagan.database.models import Ticket


class AgentOutputModal(ModalScreen[None]):
    """Modal for watching an AUTO ticket's agent progress in real-time."""

    BINDINGS = to_textual_bindings(AGENT_OUTPUT_BINDINGS)

    def __init__(
        self,
        ticket: Ticket,
        agent: Agent | None,
        iteration: int = 0,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self.ticket = ticket
        self._agent = agent
        self._iteration = iteration
        self._current_mode: str = ""
        self._available_modes: dict[str, messages.Mode] = {}
        self._available_commands: list[protocol.AvailableCommand] = []

    def compose(self) -> ComposeResult:
        with Vertical(id="agent-output-container"):
            yield Label(
                f"AUTO: {self.ticket.title[:MODAL_TITLE_MAX_LENGTH]}",
                classes="modal-title",
            )
            yield Label(
                f"Ticket #{self.ticket.short_id} | Iteration {self._iteration}",
                classes="modal-subtitle",
            )
            yield Rule()
            yield StreamingOutput(id="agent-output")
            yield Rule()
            yield Label(
                "[c] Cancel (stops + moves to Backlog)  [Esc] Close (agent continues)",
                classes="modal-hint",
            )
            with Horizontal(classes="button-row"):
                yield Button("Cancel Agent", variant="error", id="cancel-btn")
                yield Button("Close", id="close-btn")
        yield Footer()

    async def on_mount(self) -> None:
        """Set up agent message target when modal mounts."""
        output = self._get_output()
        if self._agent:
            self._agent.set_message_target(self)
            await output.post_note("Connected to agent stream", classes="info")
        else:
            await output.post_note("No agent currently running", classes="warning")

    def on_unmount(self) -> None:
        """Remove message target when modal closes."""
        if self._agent:
            self._agent.set_message_target(None)

    def _get_output(self) -> StreamingOutput:
        """Get the streaming output widget."""
        return self.query_one("#agent-output", StreamingOutput)

    # ACP Message handlers

    @on(messages.AgentUpdate)
    async def on_agent_update(self, message: messages.AgentUpdate) -> None:
        """Handle agent text output."""
        await self._get_output().post_response(message.text)

    @on(messages.Thinking)
    async def on_agent_thinking(self, message: messages.Thinking) -> None:
        """Handle agent thinking/reasoning."""
        await self._get_output().post_thought(message.text)

    @on(messages.ToolCall)
    async def on_tool_call(self, message: messages.ToolCall) -> None:
        """Handle tool call start."""
        tool_id = str(message.tool_call.get("id", "unknown"))
        title = message.tool_call.get("title", "Tool call")
        kind = message.tool_call.get("kind", "")
        await self._get_output().post_tool_call(tool_id, str(title), str(kind))

    @on(messages.ToolCallUpdate)
    async def on_tool_call_update(self, message: messages.ToolCallUpdate) -> None:
        """Handle tool call update."""
        tool_id = str(message.update.get("id", "unknown"))
        status = str(message.update.get("status", ""))
        if status:
            self._get_output().update_tool_status(tool_id, status)

    @on(messages.AgentReady)
    async def on_agent_ready(self, message: messages.AgentReady) -> None:
        """Handle agent ready."""
        await self._get_output().post_note("Agent ready", classes="success")

    @on(messages.AgentFail)
    async def on_agent_fail(self, message: messages.AgentFail) -> None:
        """Handle agent failure."""
        output = self._get_output()
        await output.post_note(f"Error: {message.message}", classes="error")
        if message.details:
            await output.post_note(message.details)

    @on(messages.Plan)
    async def on_plan(self, message: messages.Plan) -> None:
        """Display plan entries from agent."""
        await self._get_output().post_plan(message.entries)

    @on(messages.SetModes)
    def on_set_modes(self, message: messages.SetModes) -> None:
        """Store available modes from agent."""
        self._current_mode = message.current_mode
        self._available_modes = message.modes

    @on(messages.ModeUpdate)
    def on_mode_update(self, message: messages.ModeUpdate) -> None:
        """Track mode changes from agent."""
        self._current_mode = message.current_mode

    @on(messages.AvailableCommandsUpdate)
    def on_commands_update(self, message: messages.AvailableCommandsUpdate) -> None:
        """Store available slash commands from agent."""
        self._available_commands = message.commands

    @on(messages.RequestPermission)
    def on_request_permission(self, message: messages.RequestPermission) -> None:
        """Auto-approve permissions in watch mode (passive observation).

        Watch modal is for observation only - it should not block the agent
        or require user interaction for permissions.
        """
        # Find allow_once option (prefer) or allow_always
        for opt in message.options:
            if opt["kind"] == "allow_once":
                message.result_future.set_result(Answer(opt["optionId"]))
                return
        for opt in message.options:
            if "allow" in opt["kind"]:
                message.result_future.set_result(Answer(opt["optionId"]))
                return
        # Fallback to first option if no allow options exist
        if message.options:
            message.result_future.set_result(Answer(message.options[0]["optionId"]))

    # Button handlers

    @on(Button.Pressed, "#cancel-btn")
    async def on_cancel_btn(self) -> None:
        """Cancel the agent."""
        await self.action_cancel_agent()

    @on(Button.Pressed, "#close-btn")
    def on_close_btn(self) -> None:
        """Close the modal."""
        self.action_close()

    # Actions

    async def action_cancel_agent(self) -> None:
        """Stop agent completely and move ticket to BACKLOG."""
        app = cast("KaganApp", self.app)
        scheduler = app.scheduler

        if scheduler.is_running(self.ticket.id):
            # Stop both the agent process and the ticket loop task
            await scheduler.stop_ticket(self.ticket.id)
            await self._get_output().post_note("Agent stopped", classes="warning")

            # Move ticket to BACKLOG to prevent auto-restart
            from kagan.database.models import TicketStatus

            await app.state_manager.move_ticket(self.ticket.id, TicketStatus.BACKLOG)
            await self._get_output().post_note(
                "Ticket moved to BACKLOG (agent won't auto-restart)", classes="info"
            )
            self.notify("Agent stopped, ticket moved to BACKLOG")
        else:
            self.notify("No agent running for this ticket", severity="warning")

    def action_close(self) -> None:
        """Close the modal (agent continues running in background)."""
        self.dismiss(None)

    def action_copy(self) -> None:
        """Copy agent output content to clipboard."""
        output = self._get_output()
        content = output.get_text_content()
        copy_with_notification(self.app, content, "Agent output")
