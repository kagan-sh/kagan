"""Modal for viewing live agent output."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from textual import on, work
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Label, RichLog, Static

from kagan.acp import messages

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.acp.agent import Agent


class AgentOutputModal(ModalScreen[None]):
    """Modal for viewing live agent output.

    Displays streaming output from an ACP agent, including:
    - Agent text responses
    - Thinking/reasoning content
    - Tool call notifications
    - Terminal command output (via AgentUpdate messages)
    """

    BINDINGS = [
        Binding("escape", "close", "Close"),
        Binding("ctrl+c", "cancel", "Cancel"),
        Binding("ctrl+x", "terminate", "Terminate"),
    ]

    def __init__(
        self, agent: Agent, ticket_id: str, project_root: Path | None = None, **kwargs
    ) -> None:
        super().__init__(**kwargs)
        self.agent = agent
        self.ticket_id = ticket_id
        self.project_root = project_root or Path.cwd()
        self._is_running = True

    def compose(self) -> ComposeResult:
        with Vertical(id="agent-output-container"):
            yield Label(f"Agent Output: {self.ticket_id}", classes="modal-title")
            yield Static(self._status_text(), id="agent-status")
            yield RichLog(id="output-log", wrap=True, highlight=True, markup=True)
            with Horizontal(classes="button-row"):
                yield Button("Close", id="close-btn")
                yield Button("Cancel", id="cancel-btn", variant="warning")
                yield Button("Stop", id="stop-btn", variant="error")

    def on_mount(self) -> None:
        """Set ourselves as the message target when mounted."""
        self.agent.set_message_target(self)

    def on_unmount(self) -> None:
        """Clear ourselves as message target when unmounted."""
        self.agent.set_message_target(None)

    def _status_text(self) -> str:
        """Get current status text."""
        if self._is_running:
            return "Status: [bold green]RUNNING[/]"
        return "Status: [bold blue]STOPPED[/]"

    def _update_status(self) -> None:
        """Update the status display."""
        self.query_one("#agent-status", Static).update(self._status_text())

    def _append_output(self, text: str, style: str = "") -> None:
        """Append text to the output log."""
        log = self.query_one("#output-log", RichLog)
        if style:
            log.write(f"[{style}]{text}[/{style}]")
        else:
            log.write(text)
        log.scroll_end(animate=False)

    # Message handlers for ACP agent events

    @on(messages.AgentUpdate)
    def on_agent_update(self, message: messages.AgentUpdate) -> None:
        """Handle agent text output (including terminal info)."""
        # Style based on message type
        if message.type == "terminal":
            self._append_output(f"[bold yellow]{message.text}[/bold yellow]")
        elif message.type == "terminal_output":
            self._append_output(f"[dim]{message.text}[/dim]")
        elif message.type == "terminal_exit":
            self._append_output(message.text)
        else:
            self._append_output(message.text)

    @on(messages.Thinking)
    def on_agent_thinking(self, message: messages.Thinking) -> None:
        """Handle agent thinking/reasoning."""
        self._append_output(message.text, style="dim italic")

    @on(messages.ToolCall)
    def on_tool_call(self, message: messages.ToolCall) -> None:
        """Handle tool call start."""
        title = message.tool_call.get("title", "Tool call")
        kind = message.tool_call.get("kind", "")
        self._append_output(f"\n[bold cyan]> {title}[/bold cyan]", style="")
        if kind:
            self._append_output(f"  [dim]({kind})[/dim]", style="")

    @on(messages.ToolCallUpdate)
    def on_tool_call_update(self, message: messages.ToolCallUpdate) -> None:
        """Handle tool call update."""
        status = message.update.get("status")
        if status:
            style = "green" if status == "completed" else "yellow"
            self._append_output(f"  [{style}]{status}[/{style}]")

    @on(messages.AgentReady)
    def on_agent_ready(self, message: messages.AgentReady) -> None:
        """Handle agent ready."""
        self._append_output("[green]Agent ready[/green]\n")

    @on(messages.AgentFail)
    def on_agent_fail(self, message: messages.AgentFail) -> None:
        """Handle agent failure."""
        self._is_running = False
        self._update_status()
        self._append_output(f"[red bold]Error: {message.message}[/red bold]")
        if message.details:
            self._append_output(f"[red]{message.details}[/red]")

    @work
    @on(messages.RequestPermission)
    async def on_request_permission(self, message: messages.RequestPermission) -> None:
        """Handle permission request from agent."""
        from kagan.ui.modals.permission import PermissionModal

        self._append_output("[yellow]Agent requesting permission...[/yellow]")

        # Show permission modal and wait for result
        modal = PermissionModal(message.options, message.tool_call)
        result = await self.app.push_screen_wait(modal)

        if result is None:
            # User cancelled - use first reject option or create a cancel response
            for opt in message.options:
                if "reject" in opt.get("kind", ""):
                    message.result_future.set_result(messages.Answer(opt["optionId"]))
                    self._append_output("[red]Permission rejected[/red]")
                    return
            # No reject option found, just use first option
            if message.options:
                message.result_future.set_result(messages.Answer(message.options[0]["optionId"]))
            else:
                # Shouldn't happen, but handle gracefully
                message.result_future.set_exception(RuntimeError("No permission options"))
        else:
            message.result_future.set_result(result)
            self._append_output("[green]Permission granted[/green]")

    # Actions

    async def action_cancel(self) -> None:
        """Cancel the current operation."""
        await self.agent.cancel()
        self.notify("Sent cancel request")

    async def action_terminate(self) -> None:
        """Terminate the agent."""
        await self.agent.stop()
        self._is_running = False
        self._update_status()
        self.notify("Agent terminated")

    def action_close(self) -> None:
        """Close the modal."""
        self.dismiss(None)

    @on(Button.Pressed)
    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        match event.button.id:
            case "close-btn":
                self.action_close()
            case "cancel-btn":
                self.run_worker(self.action_cancel())
            case "stop-btn":
                self.run_worker(self.action_terminate())
