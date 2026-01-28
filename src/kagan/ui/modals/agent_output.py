"""Modal for watching AUTO ticket agent progress."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual import on
from textual.binding import Binding
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Label, Rule

from kagan.acp import messages
from kagan.ui.widgets import StreamingOutput

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.acp.agent import Agent
    from kagan.database.models import Ticket


class AgentOutputModal(ModalScreen[None]):
    """Modal for watching an AUTO ticket's agent progress in real-time."""

    BINDINGS = [
        Binding("escape", "close", "Close"),
        Binding("c", "cancel_agent", "Cancel Agent", show=True),
    ]

    def __init__(
        self,
        ticket: Ticket,
        agent: Agent | None,
        iteration: int = 0,
        **kwargs,
    ) -> None:
        """Initialize the modal.

        Args:
            ticket: The ticket being processed.
            agent: The ACP agent instance (may be None if not running).
            iteration: Current iteration number.
        """
        super().__init__(**kwargs)
        self.ticket = ticket
        self._agent = agent
        self._iteration = iteration

    def compose(self) -> ComposeResult:
        """Compose the modal layout."""
        with Vertical(id="agent-output-container"):
            yield Label(
                f"⚡ AUTO: {self.ticket.title[:40]}",
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
                "[c] Cancel Agent  [Esc] Close (agent continues)",
                classes="modal-hint",
            )
            with Vertical(classes="button-row"):
                yield Button("Cancel Agent", variant="error", id="cancel-btn")
                yield Button("Close", id="close-btn")
        yield Footer()

    def on_mount(self) -> None:
        """Set up agent message target when modal mounts."""
        if self._agent:
            self._agent.set_message_target(self)
            self._write_to_output("[green]Connected to agent stream[/green]\n")
        else:
            self._write_to_output("[yellow]No agent currently running[/yellow]\n")

    def on_unmount(self) -> None:
        """Remove message target when modal closes."""
        if self._agent:
            self._agent.set_message_target(None)

    def _get_output(self) -> StreamingOutput:
        """Get the streaming output widget."""
        return self.query_one("#agent-output", StreamingOutput)

    def _write_to_output(self, text: str) -> None:
        """Write text to the output widget."""
        output = self._get_output()
        self.call_later(output.write, text)

    # ACP Message handlers

    @on(messages.AgentUpdate)
    def on_agent_update(self, message: messages.AgentUpdate) -> None:
        """Handle agent text output."""
        self._write_to_output(message.text)

    @on(messages.Thinking)
    def on_agent_thinking(self, message: messages.Thinking) -> None:
        """Handle agent thinking/reasoning."""
        self._write_to_output(f"[dim italic]{message.text}[/dim italic]")

    @on(messages.ToolCall)
    def on_tool_call(self, message: messages.ToolCall) -> None:
        """Handle tool call start."""
        title = message.tool_call.get("title", "Tool call")
        kind = message.tool_call.get("kind", "")
        self._write_to_output(f"\n[bold cyan]> {title}[/bold cyan]")
        if kind:
            self._write_to_output(f"  [dim]({kind})[/dim]")

    @on(messages.ToolCallUpdate)
    def on_tool_call_update(self, message: messages.ToolCallUpdate) -> None:
        """Handle tool call update."""
        status = message.update.get("status")
        if status:
            style = "green" if status == "completed" else "yellow"
            self._write_to_output(f"  [{style}]{status}[/{style}]")

    @on(messages.AgentReady)
    def on_agent_ready(self, message: messages.AgentReady) -> None:
        """Handle agent ready."""
        self._write_to_output("[green]Agent ready[/green]\n")

    @on(messages.AgentFail)
    def on_agent_fail(self, message: messages.AgentFail) -> None:
        """Handle agent failure."""
        self._write_to_output(f"\n[red bold]Error: {message.message}[/red bold]")
        if message.details:
            self._write_to_output(f"\n[red]{message.details}[/red]")

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
        """Send cancel signal to agent."""
        if self._agent:
            await self._agent.cancel()
            self._write_to_output("\n[yellow]Cancel signal sent[/yellow]\n")
            self.notify("Sent cancel request to agent")
        else:
            self.notify("No agent to cancel", severity="warning")

    def action_close(self) -> None:
        """Close the modal (agent continues running in background)."""
        self.dismiss(None)
