"""Tmux gateway modal - info display before entering tmux session."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Container, Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Footer, Label, Rule, Static

from kagan.keybindings import TMUX_GATEWAY_BINDINGS, to_textual_bindings
from kagan.ui.utils.clipboard import copy_with_notification

if TYPE_CHECKING:
    from textual.app import ComposeResult
    from textual.events import Click

TMUX_DOCS_URL = "https://github.com/tmux/tmux/wiki"


class CopyableLink(Static):
    """Clickable link that copies URL to clipboard."""

    DEFAULT_CLASSES = "tmux-link"

    def __init__(self, url: str) -> None:
        super().__init__(f"[link]{url}[/link]")
        self._url = url

    async def _on_click(self, event: Click) -> None:
        """Copy URL on click."""
        event.stop()
        copy_with_notification(self.app, self._url, "URL")


class TmuxGatewayModal(ModalScreen[str | None]):
    """Gateway modal showing tmux info before entering session.

    Returns:
        "proceed" - User wants to continue
        "skip_future" - User wants to continue AND skip in future
        None - User cancelled
    """

    BINDINGS = to_textual_bindings(TMUX_GATEWAY_BINDINGS)

    def __init__(self, ticket_id: str, ticket_title: str, **kwargs) -> None:
        super().__init__(**kwargs)
        self._ticket_id = ticket_id
        self._ticket_title = ticket_title

    def compose(self) -> ComposeResult:
        with Container(id="tmux-gateway-container"):
            yield Label("Entering tmux Session", classes="modal-title")
            yield Rule()

            yield Static(
                "You are about to enter a [bold]tmux[/bold] terminal session.\n"
                "Kagan keybindings will be unavailable until you return.",
                classes="tmux-intro",
            )

            yield Rule(line_style="heavy")
            yield Label("Essential Commands", classes="section-title")

            with Vertical(classes="hotkey-list"):
                yield self._hotkey_row("Ctrl+b d", "Detach (return to Kagan)")
                yield self._hotkey_row("Ctrl+b c", "Create new window")
                yield self._hotkey_row("Ctrl+b n/p", "Next / previous window")
                yield self._hotkey_row("Ctrl+b %", "Split pane vertically")
                yield self._hotkey_row('Ctrl+b "', "Split pane horizontally")
                yield self._hotkey_row("Ctrl+b ?", "Show all tmux bindings")

            yield Rule()
            yield CopyableLink(TMUX_DOCS_URL)

            yield Static(
                "Press [bold]Enter[/bold] to continue  "
                "[bold]s[/bold] skip in future  "
                "[bold]Esc[/bold] cancel",
                classes="tmux-hint",
            )
        yield Footer()

    def _hotkey_row(self, key: str, description: str) -> Horizontal:
        """Create a hotkey-description row."""
        return Horizontal(
            Static(key, classes="tmux-key"),
            Static(description, classes="tmux-desc"),
            classes="tmux-hotkey-row",
        )

    def on_click(self, event: Click) -> None:
        """Dismiss when clicking outside the modal container."""
        try:
            container = self.query_one("#tmux-gateway-container")
            if not container.region.contains(event.screen_x, event.screen_y):
                self.dismiss(None)
        except Exception:
            pass

    def action_proceed(self) -> None:
        """Continue to tmux session."""
        self.dismiss("proceed")

    def action_cancel(self) -> None:
        """Cancel and return to board."""
        self.dismiss(None)

    def action_skip_future(self) -> None:
        """Continue and skip this modal in future."""
        self.dismiss("skip_future")
