"""Modal for handling missing agent with fallback options."""

from __future__ import annotations

from typing import TYPE_CHECKING, Final

from textual.containers import Container, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Label, OptionList, Static
from textual.widgets.option_list import Option

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.builtin_agents import BuiltinAgent


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

                yield Button("Cancel", variant="default", id="cancel-btn")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "install-btn":
            self._show_install_modal()
        elif event.button.id == "cancel-btn":
            self.dismiss(None)

    def _show_install_modal(self) -> None:
        from kagan.ui.modals.agent_install import AgentInstallModal

        def on_install_result(result: bool | None) -> None:
            if result is True:
                self.dismiss(AgentChoiceResult.INSTALLED)

        if self._missing:
            self.app.push_screen(
                AgentInstallModal(agent=self._missing),
                callback=on_install_result,
            )

    def on_option_list_option_selected(self, event: OptionList.OptionSelected) -> None:
        if event.option.id:
            self.dismiss(AgentChoiceResult.fallback(event.option.id))

    def action_cancel(self) -> None:
        self.dismiss(None)
