"""Install and agent selection modals for troubleshooting."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.binding import Binding, BindingType
from textual.containers import Container
from textual.screen import ModalScreen
from textual.widgets import Footer, Label, LoadingIndicator, Select
from textual.widgets._select import NoSelection

from kagan.keybindings import INSTALL_MODAL_BINDINGS

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.agents.installer import AgentType


AGENT_SELECT_MODAL_BINDINGS: list[BindingType] = [
    Binding("escape", "cancel", "Cancel"),
    Binding("enter", "select", "Select"),
]


class AgentSelectModal(ModalScreen[str | None]):
    """Modal for selecting which agent to install."""

    BINDINGS = AGENT_SELECT_MODAL_BINDINGS

    def __init__(self, agents: list[str], **kwargs) -> None:
        super().__init__(**kwargs)
        self._agents = agents

    def compose(self) -> ComposeResult:
        from kagan.data.builtin_agents import get_builtin_agent

        with Container(id="agent-select-modal"):
            yield Label("Select Agent to Install", classes="install-modal-title")
            options: list[tuple[str, str]] = []
            for agent_id in self._agents:
                info = get_builtin_agent(agent_id)
                name = info.config.name if info else agent_id.title()
                options.append((name, agent_id))
            yield Select[str](
                options,
                id="agent-select",
                value=self._agents[0] if self._agents else NoSelection(),
            )
            yield Label(
                "Press Enter to select, Escape to cancel",
                classes="install-modal-hint",
            )
        yield Footer()

    def action_select(self) -> None:
        """Select the chosen agent."""
        select = self.query_one("#agent-select", Select)
        self.dismiss(str(select.value) if select.value else None)

    def action_cancel(self) -> None:
        """Cancel and close the modal."""
        self.dismiss(None)


class InstallModal(ModalScreen[bool]):
    """Modal for installing an AI agent."""

    BINDINGS = INSTALL_MODAL_BINDINGS

    def __init__(self, agent: AgentType = "claude", **kwargs) -> None:
        super().__init__(**kwargs)
        self._agent: AgentType = agent
        self._is_installing = False
        self._install_complete = False
        self._install_success = False
        self._result_message = ""

    def compose(self) -> ComposeResult:
        from kagan.agents.installer import get_install_command
        from kagan.data.builtin_agents import get_builtin_agent

        agent_info = get_builtin_agent(self._agent)
        agent_name = agent_info.config.name if agent_info else self._agent.title()
        install_cmd = get_install_command(self._agent)

        with Container(id="install-modal-container"):
            yield Label(f"Install {agent_name}", classes="install-modal-title")
            yield Label(
                "This will run the installation command:",
                classes="install-modal-subtitle",
            )
            yield Label(
                f"$ {install_cmd}",
                id="install-command",
                classes="install-modal-command",
            )
            yield LoadingIndicator(id="install-spinner")
            yield Label("", id="install-status", classes="install-modal-status")
            yield Label(
                "Press Enter to install, Escape to cancel",
                id="install-hint",
                classes="install-modal-hint",
            )
        yield Footer()

    def on_mount(self) -> None:
        """Hide spinner initially."""
        self.query_one("#install-spinner", LoadingIndicator).display = False

    async def action_install(self) -> None:
        """Start the installation process."""
        if self._is_installing or self._install_complete:
            return

        from kagan.data.builtin_agents import get_builtin_agent

        self._is_installing = True
        spinner = self.query_one("#install-spinner", LoadingIndicator)
        status = self.query_one("#install-status", Label)
        hint = self.query_one("#install-hint", Label)

        # Get agent name for display
        agent_info = get_builtin_agent(self._agent)
        agent_name = agent_info.config.name if agent_info else self._agent.title()

        # Show spinner and update status
        spinner.display = True
        status.update(f"Installing {agent_name}...")
        hint.update("Please wait...")

        # Run installation
        try:
            from kagan.agents.installer import install_agent

            success, message = await install_agent(self._agent)
            self._install_success = success
            self._result_message = message
        except Exception as e:
            self._install_success = False
            self._result_message = f"Installation error: {e}"

        # Hide spinner and show result
        spinner.display = False
        self._install_complete = True
        self._is_installing = False

        if self._install_success:
            status.add_class("success")
            status.update(f"[bold green]Success![/] {self._result_message}")
            hint.update("Press Enter to restart Kagan, Escape to close")
        else:
            status.add_class("error")
            status.update(f"[bold red]Failed:[/] {self._result_message}")
            hint.update("Press Escape to close")

    async def action_confirm(self) -> None:
        """Confirm action - install or dismiss with success."""
        if self._install_complete:
            # If installation is complete and successful, dismiss with True to signal restart
            self.dismiss(self._install_success)
        else:
            # Start installation
            await self.action_install()

    def action_cancel(self) -> None:
        """Cancel and close the modal."""
        if self._is_installing:
            # Can't cancel during installation
            self.notify("Installation in progress...", severity="warning")
            return
        self.dismiss(False)
