"""Chat screen for planner agent interaction."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, cast

from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Footer, Input, Label, RichLog, Static

from kagan.agents import AgentProcess, AgentState

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.app import KaganApp

PLANNER_SESSION_ID = "planner"


class ChatScreen(Screen):
    """Chat screen for planner agent interaction."""

    BINDINGS = [
        Binding("escape", "back", "Back"),
        Binding("ctrl+c", "interrupt", "Interrupt", show=False),
    ]

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self._agent: AgentProcess | None = None
        self._timer = None

    @property
    def kagan_app(self) -> KaganApp:
        return cast("KaganApp", self.app)

    def compose(self) -> ComposeResult:
        with Vertical(id="chat-container"):
            yield Label("Planner Chat", id="chat-header")
            yield Static("Status: Not started", id="chat-status")
            yield RichLog(id="chat-log", wrap=True, highlight=True, markup=True)
            with Horizontal(id="chat-input-row"):
                yield Input(placeholder="Describe your goal...", id="chat-input")
        yield Footer()

    async def on_mount(self) -> None:
        await self._start_planner()
        self.query_one("#chat-input", Input).focus()

    async def _start_planner(self) -> None:
        """Start the planner agent."""
        manager = self.kagan_app.agent_manager

        # Check if already running
        existing = manager.get(PLANNER_SESSION_ID)
        if existing and existing.state == AgentState.RUNNING:
            self._agent = existing
            self._start_output_timer()
            return

        # Get planner command from config
        config = self.kagan_app.config
        hat = config.get_hat("planner")
        if hat:
            command = hat.agent_command
            if hat.args:
                command += " " + " ".join(hat.args)
        else:
            command = "claude"

        # Spawn in current directory (no worktree for planner)
        cwd = Path.cwd()

        try:
            self._agent = await manager.spawn(PLANNER_SESSION_ID, command, cwd)
            self._update_status()
            self._start_output_timer()
        except ValueError:
            self._agent = manager.get(PLANNER_SESSION_ID)
            if self._agent:
                self._start_output_timer()

    def _start_output_timer(self) -> None:
        """Start polling for agent output."""
        if self._timer is None:
            self._timer = self.set_interval(0.3, self._poll_output)

    def _poll_output(self) -> None:
        """Poll and display agent output."""
        if not self._agent:
            return
        output, _ = self._agent.get_output()
        log = self.query_one("#chat-log", RichLog)
        log.clear()
        log.write(output)
        log.scroll_end(animate=False)
        self._update_status()

    def _update_status(self) -> None:
        """Update status display."""
        status = self.query_one("#chat-status", Static)
        if not self._agent:
            status.update("Status: Not started")
            return
        state = self._agent.state
        match state:
            case AgentState.RUNNING:
                status.update("Status: [bold green]RUNNING[/]")
            case AgentState.FINISHED:
                status.update(f"Status: [bold blue]FINISHED[/] (exit: {self._agent.return_code})")
            case AgentState.FAILED:
                status.update(f"Status: [bold red]FAILED[/] (exit: {self._agent.return_code})")
            case _:
                status.update(f"Status: {state.value}")

    async def on_input_submitted(self, event: Input.Submitted) -> None:
        """Handle user input submission."""
        text = event.value.strip()
        if not text:
            return

        event.input.value = ""

        log = self.query_one("#chat-log", RichLog)
        log.write(f"\n[bold cyan]You:[/] {text}\n")

        if self._agent and self._agent.state == AgentState.RUNNING:
            await self._agent.send_input(text + "\n")
        else:
            self.notify("Planner not running", severity="warning")

    async def action_interrupt(self) -> None:
        """Send interrupt signal to planner."""
        if self._agent and self._agent.state == AgentState.RUNNING:
            await self._agent.interrupt()
            self.notify("Sent interrupt (Ctrl+C)")

    def action_back(self) -> None:
        """Return to Kanban screen."""
        if self._timer:
            self._timer.stop()
            self._timer = None
        self.app.pop_screen()

    async def on_unmount(self) -> None:
        """Cleanup on screen exit."""
        if self._timer:
            self._timer.stop()
            self._timer = None
