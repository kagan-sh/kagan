"""Modal for viewing live agent output."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Label, RichLog, Static

from kagan.agents import AgentProcess, AgentState

if TYPE_CHECKING:
    from textual.app import ComposeResult


class AgentOutputModal(ModalScreen[None]):
    """Modal for viewing live agent output."""

    BINDINGS = [
        Binding("escape", "close", "Close"),
        Binding("r", "refresh", "Refresh"),
        Binding("ctrl+c", "interrupt", "Interrupt"),
        Binding("ctrl+x", "terminate", "Terminate"),
    ]

    def __init__(self, agent: AgentProcess, **kwargs) -> None:
        super().__init__(**kwargs)
        self.agent = agent
        self._timer = None

    def compose(self) -> ComposeResult:
        with Vertical(id="agent-output-container"):
            yield Label(f"Agent Output: {self.agent.ticket_id}", classes="modal-title")
            yield Static(self._status_text(), id="agent-status")
            yield RichLog(id="output-log", wrap=True, highlight=True, markup=True)
            with Horizontal(classes="button-row"):
                yield Button("Close", id="close-btn")
                yield Button("Refresh", id="refresh-btn")
                yield Button("Interrupt", id="interrupt-btn", variant="warning")
                yield Button("Stop", id="stop-btn", variant="error")

    def on_mount(self) -> None:
        self._update()
        self._timer = self.set_interval(0.5, self._on_timer_tick)

    def _status_text(self) -> str:
        state, code = self.agent.state, self.agent.return_code
        match state:
            case AgentState.RUNNING:
                return "Status: [bold green]RUNNING[/]"
            case AgentState.FINISHED:
                return f"Status: [bold blue]FINISHED[/] (exit: {code})"
            case AgentState.FAILED:
                return f"Status: [bold red]FAILED[/] (exit: {code})"
            case _:
                return f"Status: {state.value}"

    def _update(self) -> None:
        log = self.query_one("#output-log", RichLog)
        output, truncated = self.agent.get_output()
        log.clear()
        if truncated:
            log.write("[dim](output truncated)[/dim]\n")
        log.write(output)
        log.scroll_end(animate=False)
        self.query_one("#agent-status", Static).update(self._status_text())

    def _on_timer_tick(self) -> None:
        if self.agent.state == AgentState.RUNNING:
            self._update()
        elif self._timer:
            self._timer.stop()

    def action_refresh(self) -> None:
        self._update()

    async def action_interrupt(self) -> None:
        await self.agent.interrupt()
        self.notify("Sent interrupt (Ctrl+C)")

    async def action_terminate(self) -> None:
        await self.agent.terminate()
        self._update()
        self.notify("Agent terminated")

    def action_close(self) -> None:
        if self._timer:
            self._timer.stop()
        self.dismiss(None)

    def on_button_pressed(self, event: Button.Pressed) -> None:
        match event.button.id:
            case "close-btn":
                self.action_close()
            case "refresh-btn":
                self.action_refresh()
            case "interrupt-btn":
                self.run_worker(self.action_interrupt())
            case "stop-btn":
                self.run_worker(self.action_terminate())
