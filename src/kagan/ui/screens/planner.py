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
from kagan.agents.planner import build_planner_prompt, parse_ticket_from_response
from kagan.agents.prompt_loader import PromptLoader
from kagan.config import get_fallback_agent_config
from kagan.ui.screens.base import KaganScreen
from kagan.ui.widgets import StreamingOutput

if TYPE_CHECKING:
    from textual.app import ComposeResult

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

    def compose(self) -> ComposeResult:
        """Compose the planner screen layout."""
        with Vertical(id="planner-container"):
            yield Static("What do you want to build?", id="planner-header")
            yield StreamingOutput(id="planner-output")
            yield Input(placeholder="Describe your feature or task...", id="planner-input")
        yield Footer()

    async def on_mount(self) -> None:
        """Start planner agent and focus input on mount."""
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
        await output.write(f"\n[bold cyan]> {title}[/bold cyan]")
        if kind:
            await output.write(f"  [dim]({kind})[/dim]")

    @on(messages.ToolCallUpdate)
    async def on_tool_call_update(self, message: messages.ToolCallUpdate) -> None:
        """Handle tool call update."""
        status = message.update.get("status")
        if status:
            style = "green" if status == "completed" else "yellow"
            output = self._get_output()
            await output.write(f"  [{style}]{status}[/{style}]")

    @on(messages.AgentReady)
    async def on_agent_ready(self, message: messages.AgentReady) -> None:
        """Handle agent ready."""
        await self._get_output().write("[green]Agent ready[/green]\n")

    @on(messages.AgentFail)
    async def on_agent_fail(self, message: messages.AgentFail) -> None:
        """Handle agent failure."""
        self._is_running = False
        self._update_status()
        output = self._get_output()
        await output.write(f"[red bold]Error: {message.message}[/red bold]")
        if message.details:
            await output.write(f"[red]{message.details}[/red]")

    async def _try_create_ticket_from_response(self) -> None:
        """Parse accumulated response and create ticket if found."""
        if not self._accumulated_response:
            return

        full_response = "".join(self._accumulated_response)
        parsed = parse_ticket_from_response(full_response)

        if parsed is None:
            return

        try:
            ticket = await self.kagan_app.state_manager.create_ticket(parsed.ticket)
            self.notify(
                f"Created ticket [{ticket.short_id}]: {ticket.title[:50]}",
                severity="information",
            )
            await self._get_output().write(
                f"\n[bold green]✓ Created ticket {ticket.short_id}[/bold green]\n"
            )
            # After creating ticket, navigate to board
            await self.action_to_board()
        except Exception as e:
            self.notify(f"Failed to create ticket: {e}", severity="error")
            await self._get_output().write(f"[red]Failed to create ticket: {e}[/red]")

    async def on_input_submitted(self, event: Input.Submitted) -> None:
        """Handle user input submission."""
        text = event.value.strip()
        if not text:
            return

        event.input.value = ""

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
                await self._agent.wait_ready(timeout=30.0)
                await self._agent.send_prompt(prompt)
                # After prompt completes, try to create ticket from response
                await self._try_create_ticket_from_response()
            except Exception as e:
                await self._get_output().write(f"[red]Error sending prompt: {e}[/red]")

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
