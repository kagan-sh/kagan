"""Terminal installation modal for PAIR backends."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from textual.containers import Container, Horizontal
from textual.screen import ModalScreen
from textual.widgets import Button, Label, LoadingIndicator, Static

from kagan.tui.terminals.installer import get_manual_install_fallback

if TYPE_CHECKING:
    from textual.app import ComposeResult


class TerminalInstallModal(ModalScreen[bool]):
    """Prompt user to install terminal backend.

    Returns:
        True when install succeeds, False otherwise.
    """

    BINDINGS = [("escape", "cancel", "Cancel")]

    def __init__(self, backend: str = "tmux") -> None:
        super().__init__()
        self._backend = backend
        self._installing = False

    def compose(self) -> ComposeResult:
        title = "Install tmux" if self._backend == "tmux" else "Install Terminal"
        fallback = get_manual_install_fallback(self._backend)

        with Container(id="terminal-install-container"):
            yield Label(title, classes="modal-title")
            yield Static(
                "PAIR session requires this terminal backend before opening the session.",
                id="terminal-install-description",
            )
            yield Static(fallback, id="terminal-install-fallback")
            yield LoadingIndicator(id="terminal-install-spinner")
            yield Static("", id="terminal-install-status")

            with Horizontal(id="terminal-install-buttons"):
                yield Button("Install", variant="primary", id="install-btn")
                yield Button("Cancel", id="cancel-btn")

    def on_mount(self) -> None:
        self.query_one("#terminal-install-spinner", LoadingIndicator).display = False
        self.query_one("#terminal-install-status", Static).display = False

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "install-btn":
            self._do_install()
        elif event.button.id == "cancel-btn":
            self.dismiss(False)

    def _do_install(self) -> None:
        if self._installing:
            return

        self._installing = True
        self.query_one("#terminal-install-buttons", Horizontal).display = False
        self.query_one("#terminal-install-spinner", LoadingIndicator).display = True
        status = self.query_one("#terminal-install-status", Static)
        status.update("Installing...")
        status.display = True
        self.run_worker(self._run_install(), group="terminal-install")

    async def _run_install(self) -> None:
        from kagan.tui.terminals.installer import install_terminal

        success, message = await install_terminal(self._backend)

        self.query_one("#terminal-install-spinner", LoadingIndicator).display = False
        status = self.query_one("#terminal-install-status", Static)

        if success:
            status.update(f"[green]{message}[/green]")
            await asyncio.sleep(1.2)
            self.dismiss(True)
            return

        status.update(f"[red]{message}[/red]")
        self.query_one("#terminal-install-buttons", Horizontal).display = True
        self.query_one("#install-btn", Button).label = "Retry"
        self._installing = False

    def action_cancel(self) -> None:
        if not self._installing:
            self.dismiss(False)
