"""Modal for handling missing agent with fallback options."""

from __future__ import annotations

from contextlib import suppress
from typing import TYPE_CHECKING, Final

from textual import on
from textual.containers import Container, Vertical
from textual.css.query import NoMatches
from textual.screen import ModalScreen
from textual.widgets import Button, Label, OptionList, Static
from textual.widgets.option_list import Option

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.core.builtin_agents import BuiltinAgent


class AgentChoiceResult:
    """Constants for AgentChoiceModal return values."""

    INSTALLED: Final[str] = "installed"
    CANCELLED: Final[str] = "cancelled"
    _FALLBACK_PREFIX: Final[str] = "fallback:"

    @staticmethod
    def fallback(agent: str) -> str:
        """Create fallback result for the given agent."""
        return f"{AgentChoiceResult._FALLBACK_PREFIX}{agent}"

    @staticmethod
    def parse_fallback(result: str) -> str | None:
        """Parse agent name from fallback result, or None if not a fallback."""
        if result.startswith(AgentChoiceResult._FALLBACK_PREFIX):
            return result[len(AgentChoiceResult._FALLBACK_PREFIX) :]
        return None


class AgentChoiceModal(ModalScreen[str | None]):
    """Modal offering install/fallback when task agent is missing.

    Returns:
        "installed"          - User installed the missing agent successfully
        "fallback:<name>"    - User chose a different agent
        None                 - User cancelled
    """

    BINDINGS = [("escape", "cancel", "Cancel")]

    def __init__(
        self,
        missing_agent: BuiltinAgent | None,
        available_agents: list[BuiltinAgent],
        task_title: str,
    ) -> None:
        super().__init__()
        self._missing = missing_agent
        self._available = available_agents
        self._task_title = task_title
        self._installing = False

    def compose(self) -> ComposeResult:
        name = self._missing.config.name if self._missing else "Unknown agent"

        with Container(id="agent-choice-container"):
            yield Label("Agent Not Available", id="agent-choice-title")
            yield Static(
                f'Task "{self._task_title}" is assigned to [bold]{name}[/bold],\n'
                "which is not installed.",
                id="agent-choice-description",
            )

            with Vertical(id="agent-choice-options"):
                if self._missing:
                    yield Button(
                        f"Install {name}",
                        variant="primary",
                        id="install-btn",
                    )

                if self._available:
                    yield Static("Or use a different agent:", id="fallback-label")
                    options = [
                        Option(f"{a.config.name}", id=a.config.short_name) for a in self._available
                    ]
                    yield OptionList(*options, id="agent-list")

                yield Static("", id="agent-choice-status")
                yield Button("Cancel", variant="default", id="cancel-btn")

    @on(Button.Pressed, "#install-btn")
    def on_install_pressed(self) -> None:
        if self._missing is None or self._installing:
            return
        self._installing = True
        self._set_inputs_disabled(True)
        self._set_status("[yellow]Installing agent...[/yellow]")
        self.run_worker(
            self._run_install(self._missing.config.short_name),
            group="agent-choice-install",
            exclusive=True,
            exit_on_error=False,
        )

    @on(Button.Pressed, "#cancel-btn")
    def on_cancel_pressed(self) -> None:
        if self._installing:
            return
        self.dismiss(None)

    async def _run_install(self, agent_short_name: str) -> None:
        from kagan.core.agents.installer import install_agent

        success, message = await install_agent(agent_short_name)
        if success:
            self._set_status(f"[green]{message}[/green]")
            self.dismiss(AgentChoiceResult.INSTALLED)
            return

        self._set_status(f"[red]{message}[/red]")
        self._installing = False
        self._set_inputs_disabled(False)

    def on_option_list_option_selected(self, event: OptionList.OptionSelected) -> None:
        """Handle option list option selected event."""
        if event.option.id:
            self.dismiss(AgentChoiceResult.fallback(event.option.id))

    def _set_status(self, message: str) -> None:
        status = self.query_one("#agent-choice-status", Static)
        status.update(message)

    def _set_inputs_disabled(self, disabled: bool) -> None:
        with suppress(NoMatches):
            self.query_one("#install-btn", Button).disabled = disabled
        self.query_one("#cancel-btn", Button).disabled = disabled
        with suppress(NoMatches):
            self.query_one("#agent-list", OptionList).disabled = disabled

    def action_cancel(self) -> None:
        if not self._installing:
            self.dismiss(None)
