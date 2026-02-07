"""MCP installation consent modal for PAIR sessions."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Container, Horizontal
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Label, LoadingIndicator, Static

from kagan.keybindings import CONFIRM_BINDINGS

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.config import AgentConfig
    from kagan.mcp.global_config import GlobalMcpSpec


class McpInstallModal(ModalScreen[bool | None]):
    """Blocking modal for MCP installation consent.

    Returns:
        True  - Install succeeded
        None  - User cancelled or install failed and user chose not to retry
    """

    BINDINGS = CONFIRM_BINDINGS

    def __init__(
        self,
        agent_config: AgentConfig,
        spec: GlobalMcpSpec,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self._agent_config = agent_config
        self._spec = spec
        self._installing = False

    def compose(self) -> ComposeResult:
        from kagan.mcp.global_config import get_install_description

        description = get_install_description(self._spec)

        with Container(id="mcp-install-container"):
            yield Label("MCP Setup Required", id="mcp-install-title")
            yield Static(
                f"Kagan needs to register its MCP tools with "
                f"[bold]{self._agent_config.name}[/bold] to enable task coordination.",
                id="mcp-install-description",
            )
            yield Static(
                f"[dim]{description}[/dim]",
                id="mcp-install-detail",
            )
            if self._spec.config_path:
                yield Static(
                    f"Config: [italic]{self._spec.config_path}[/italic]",
                    id="mcp-install-path",
                )
            with Horizontal(id="mcp-install-buttons"):
                yield Button("Install", variant="success", id="mcp-install-btn")
                yield Button("Cancel", variant="default", id="mcp-cancel-btn")
            yield LoadingIndicator(id="mcp-install-spinner")
            yield Label("", id="mcp-install-status")
        yield Footer(show_command_palette=False)

    def on_mount(self) -> None:
        self.query_one("#mcp-install-spinner", LoadingIndicator).display = False
        self.query_one("#mcp-install-status", Label).display = False

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "mcp-install-btn":
            self._do_install()
        elif event.button.id == "mcp-cancel-btn":
            self.dismiss(None)

    def action_confirm(self) -> None:
        if not self._installing:
            self._do_install()

    def action_cancel(self) -> None:
        if not self._installing:
            self.dismiss(None)

    def _do_install(self) -> None:
        if self._installing:
            return
        self._installing = True

        self.query_one("#mcp-install-btn", Button).disabled = True
        self.query_one("#mcp-cancel-btn", Button).disabled = True
        self.query_one("#mcp-install-spinner", LoadingIndicator).display = True

        status = self.query_one("#mcp-install-status", Label)
        status.update("Installing...")
        status.display = True

        self.run_worker(self._run_install(), group="mcp-install")

    async def _run_install(self) -> None:
        from kagan.mcp.global_config import install_global_mcp, is_global_mcp_configured

        agent = self._spec.agent
        success, message, _path = install_global_mcp(agent)

        if success and is_global_mcp_configured(agent):
            self.dismiss(True)
            return

        if success:
            message = "Install ran but verification failed"

        self._installing = False
        self.query_one("#mcp-install-spinner", LoadingIndicator).display = False

        status = self.query_one("#mcp-install-status", Label)
        status.update(f"[red]Failed:[/red] {message}")

        install_btn = self.query_one("#mcp-install-btn", Button)
        install_btn.label = "Retry"
        install_btn.disabled = False
        self.query_one("#mcp-cancel-btn", Button).disabled = False
