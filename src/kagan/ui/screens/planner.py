"""Planner screen for chat-first ticket creation."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from textual import on
from textual.binding import Binding
from textual.containers import Vertical
from textual.widgets import Footer, Input, Static

from kagan.acp import messages
from kagan.acp.agent import Agent
from kagan.agents.planner import build_planner_prompt, parse_plan
from kagan.agents.prompt_loader import PromptLoader
from kagan.config import get_fallback_agent_config
from kagan.constants import PLANNER_TITLE_MAX_LENGTH
from kagan.limits import AGENT_TIMEOUT
from kagan.ui.screens.approval import ApprovalScreen
from kagan.ui.screens.base import KaganScreen
from kagan.ui.widgets import EmptyState, StatusBar, StreamingOutput
from kagan.ui.widgets.header import KaganHeader

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.database.models import TicketCreate

PLANNER_SESSION_ID = "planner"


class PlannerScreen(KaganScreen):
    """Chat-first planner for creating tickets."""

    BINDINGS = [
        Binding("escape", "to_board", "Go to Board"),
        Binding("ctrl+c", "cancel", "Cancel", show=False),
    ]

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self._agent: Agent | None = None
        self._is_running = False
        self._accumulated_response: list[str] = []
        self._agent_ready = False
        self._has_agent_output = False

    def compose(self) -> ComposeResult:
        """Compose the planner screen layout."""
        yield KaganHeader()
        with Vertical(id="planner-container"):
            yield Static(
                "Plan Mode",
                id="planner-header",
            )
            yield EmptyState()
            yield StreamingOutput(id="planner-output")
            yield StatusBar()
            yield Input(
                placeholder="Describe your feature or task...", id="planner-input", disabled=True
            )
        yield Footer()

    async def on_mount(self) -> None:
        """Start planner agent and focus input on mount."""
        from contextlib import suppress

        from textual.css.query import NoMatches

        # Update status bar to show initialization
        with suppress(NoMatches):
            status_bar = self.query_one(StatusBar)
            status_bar.update_status("initializing", "Initializing agent...")

        await self._start_planner()
        self.query_one("#planner-input", Input).focus()

    async def _start_planner(self) -> None:
        """Start the planner agent."""
        # Get agent config from config (uses user's selection from welcome screen)
        config = self.kagan_app.config
        agent_config = config.get_worker_agent()

        if agent_config is None:
            agent_config = get_fallback_agent_config()

        # Spawn in current directory (no worktree for planner)
        cwd = Path.cwd()

        self._agent = Agent(cwd, agent_config)
        self._agent.start(self)
        self._is_running = True
        self._update_status()

    def _update_status(self) -> None:
        """Update status display (no-op for simplified UI)."""
        pass

    def _get_output(self) -> StreamingOutput:
        """Get the streaming output widget."""
        return self.query_one("#planner-output", StreamingOutput)

    # Message handlers for ACP agent events

    @on(messages.AgentUpdate)
    async def on_agent_update(self, message: messages.AgentUpdate) -> None:
        """Handle agent text output."""
        from contextlib import suppress

        from textual.css.query import NoMatches

        # On first update, hide EmptyState and show output
        if not self._has_agent_output:
            self._has_agent_output = True
            with suppress(NoMatches):
                empty_state = self.query_one(EmptyState)
                empty_state.add_class("hidden")
            with suppress(NoMatches):
                output = self._get_output()
                output.add_class("visible")

        self._accumulated_response.append(message.text)
        await self._get_output().write(message.text)

    @on(messages.Thinking)
    async def on_agent_thinking(self, message: messages.Thinking) -> None:
        """Handle agent thinking/reasoning."""
        # Thinking can be chunked, so use main stream with italic formatting
        await self._get_output().write(f"*{message.text}*")

    @on(messages.ToolCall)
    async def on_tool_call(self, message: messages.ToolCall) -> None:
        """Handle tool call start."""
        output = self._get_output()
        title = message.tool_call.get("title", "Tool call")
        kind = message.tool_call.get("kind", "")
        await output.write(f"\n**> {title}**")
        if kind:
            await output.write(f" *({kind})*")

    @on(messages.ToolCallUpdate)
    async def on_tool_call_update(self, message: messages.ToolCallUpdate) -> None:
        """Handle tool call update."""
        status = message.update.get("status")
        if status:
            symbol = "✓" if status == "completed" else "⋯"
            output = self._get_output()
            await output.write(f" {symbol} {status}")

    @on(messages.AgentReady)
    async def on_agent_ready(self, message: messages.AgentReady) -> None:
        """Handle agent ready."""
        from contextlib import suppress

        from textual.css.query import NoMatches

        self._agent_ready = True

        # Enable input
        input_widget = self.query_one("#planner-input", Input)
        input_widget.disabled = False

        # Update status bar
        with suppress(NoMatches):
            status_bar = self.query_one(StatusBar)
            status_bar.update_status("ready", "Press H for help")

        # Focus input
        input_widget.focus()

        # Write to output
        await self._get_output().write("**Agent ready** ✓\n")

    @on(messages.AgentFail)
    async def on_agent_fail(self, message: messages.AgentFail) -> None:
        """Handle agent failure."""
        from contextlib import suppress

        from textual.css.query import NoMatches

        self._is_running = False
        self._update_status()

        # Update status bar to error state
        with suppress(NoMatches):
            status_bar = self.query_one(StatusBar)
            status_bar.update_status("error", f"Error: {message.message}")

        # Disable input
        input_widget = self.query_one("#planner-input", Input)
        input_widget.disabled = True

        # Write error to output
        output = self._get_output()
        await output.write(f"**Error:** {message.message}")
        if message.details:
            await output.write(f"\n{message.details}")

    async def _try_create_ticket_from_response(self) -> None:
        """Parse accumulated response and show approval screen if plan found."""
        if not self._accumulated_response:
            return

        full_response = "".join(self._accumulated_response)

        # Parse as multi-ticket plan (only format supported)
        tickets = parse_plan(full_response)
        if tickets:
            # Show approval screen for tickets
            self.app.push_screen(
                ApprovalScreen(tickets),
                self._on_approval_result,
            )
        # If no <plan> block found, agent is still gathering info - continue conversation

    async def _on_approval_result(self, result: list[TicketCreate] | str | None) -> None:
        """Handle approval screen result."""
        if result is None:
            # Cancelled - clear and continue
            self._accumulated_response.clear()
            await self._get_output().clear()
            await self._get_output().write("Plan cancelled. Describe what you want to build.\n\n")
            return

        if result == "refine":
            # User wants to refine - prompt agent to adjust
            await self._get_output().write("\n**Refining plan...**\n\n")
            self._accumulated_response.clear()
            refine_prompt = "Please adjust the plan based on my feedback. What changes?"
            await self._send_prompt(refine_prompt)
            return

        # At this point, result must be list[TicketCreate] (not None or str)
        if isinstance(result, str):
            return

        # Approved - create all tickets
        created_count = 0
        for ticket_data in result:
            try:
                ticket = await self.kagan_app.state_manager.create_ticket(ticket_data)
                self.notify(
                    f"Created: {ticket.title[:PLANNER_TITLE_MAX_LENGTH]}", severity="information"
                )
                created_count += 1
            except Exception as e:
                self.notify(f"Failed to create ticket: {e}", severity="error")

        if created_count > 0:
            await self._get_output().write(f"\n**Created {created_count} ticket(s)**\n")
            await self.action_to_board()

    async def on_input_submitted(self, event: Input.Submitted) -> None:
        """Handle user input submission."""
        from contextlib import suppress

        from textual.css.query import NoMatches

        # Only submit if agent is ready
        if not self._agent_ready:
            return

        text = event.value.strip()
        if not text:
            return

        event.input.value = ""

        # Update status bar to thinking state
        with suppress(NoMatches):
            status_bar = self.query_one(StatusBar)
            status_bar.update_status("thinking", "Processing...")

        # Write user input to streaming output
        await self._get_output().write(f"\n\n**You:** {text}\n\n")

        if self._agent and self._is_running:
            # Use send_prompt for ACP agents
            self.run_worker(self._send_prompt(text))
        else:
            self.notify("Planner not running", severity="warning")

    async def _send_prompt(self, text: str) -> None:
        """Send prompt to agent asynchronously."""
        if self._agent:
            # Clear accumulated response before sending new prompt
            self._accumulated_response.clear()

            # Build planner prompt with system instructions using PromptLoader
            prompt_loader = PromptLoader(self.kagan_app.config)
            prompt = build_planner_prompt(text, prompt_loader)

            try:
                await self._agent.wait_ready(timeout=AGENT_TIMEOUT)
                await self._agent.send_prompt(prompt)
                # After prompt completes, try to create ticket from response
                await self._try_create_ticket_from_response()
            except Exception as e:
                await self._get_output().write(f"**Error sending prompt:** {e}")

    async def action_cancel(self) -> None:
        """Send cancel signal to planner."""
        if self._agent and self._is_running:
            await self._agent.cancel()
            self.notify("Sent cancel request")

    async def action_to_board(self) -> None:
        """Navigate to Kanban board screen."""
        from kagan.ui.screens.kanban import KanbanScreen

        await self.app.push_screen(KanbanScreen())

    async def on_unmount(self) -> None:
        """Cleanup on screen exit."""
        if self._agent:
            await self._agent.stop()
            self._is_running = False
