"""PAIR instructions modal shown before opening a PAIR session."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Container, Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Footer, Label, Rule, Static

from kagan.tui.keybindings import TMUX_GATEWAY_BINDINGS
from kagan.tui.ui.utils.helpers import copy_with_notification

if TYPE_CHECKING:
    from pathlib import Path

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
        event.stop()
        copy_with_notification(self.app, self._url, "URL")


class PairInstructionsModal(ModalScreen[str | None]):
    """Instructions popup shown before launching PAIR tool."""

    BINDINGS = TMUX_GATEWAY_BINDINGS

    def __init__(
        self,
        task_id: str,
        task_title: str,
        backend: str,
        prompt_path: Path,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self._task_id = task_id
        self._task_title = task_title
        self._backend = backend
        self._prompt_path = prompt_path

    def compose(self) -> ComposeResult:
        with Container(id="pair-instructions-container"):
            yield Label("PAIR Session Instructions", classes="modal-title")
            yield Rule()

            if self._backend == "tmux":
                yield Static(
                    "You are about to enter a [bold]tmux[/bold] session.\n"
                    "Kagan keybindings are paused until you detach.",
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
            else:
                tool_name = "VS Code" if self._backend == "vscode" else "Cursor"
                chat_name = "Copilot Chat" if self._backend == "vscode" else "Cursor Chat"
                yield Static(
                    f"Kagan will open [bold]{tool_name}[/bold] for this task.\n"
                    "Preferred workflow: paste Kagan startup prompt into chat first.",
                    classes="tmux-intro",
                )
                yield Rule(line_style="heavy")
                yield Label("What To Do Next", classes="section-title")
                with Vertical(classes="hotkey-list"):
                    yield Static(
                        "1. Open chat interface in the IDE (Copilot Chat/Cursor Chat).",
                        classes="tmux-desc",
                    )
                    yield Static(
                        f"2. Open startup prompt file:\n{self._prompt_path}",
                        classes="tmux-desc",
                    )
                    yield Static(
                        f"3. Copy prompt contents and paste into {chat_name}.",
                        classes="tmux-desc",
                    )
                    yield Static(
                        "4. Agent uses Kagan MCP and should move the task to REVIEW when done.",
                        classes="tmux-desc",
                    )

            yield Static(
                "Press [bold]Enter[/bold] to continue  "
                "[bold]s[/bold] skip in future  "
                "[bold]Esc[/bold] cancel",
                classes="tmux-hint",
            )
        yield Footer(show_command_palette=False)

    def _hotkey_row(self, key: str, description: str) -> Horizontal:
        return Horizontal(
            Static(key, classes="tmux-key"),
            Static(description, classes="tmux-desc"),
            classes="tmux-hotkey-row",
        )

    def on_click(self, event: Click) -> None:
        try:
            container = self.query_one("#pair-instructions-container")
            if not container.region.contains(event.screen_x, event.screen_y):
                self.dismiss(None)
        except Exception:
            pass

    def action_proceed(self) -> None:
        self.dismiss("proceed")

    def action_cancel(self) -> None:
        self.dismiss(None)

    def action_skip_future(self) -> None:
        self.dismiss("skip_future")


TmuxGatewayModal = PairInstructionsModal
