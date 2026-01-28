"""Modal for viewing git diffs."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.binding import Binding
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Label, RichLog

if TYPE_CHECKING:
    from textual.app import ComposeResult


class DiffModal(ModalScreen[None]):
    """Modal for showing a ticket diff."""

    BINDINGS = [
        Binding("escape", "close", "Close"),
    ]

    def __init__(self, title: str, diff_text: str, **kwargs) -> None:
        super().__init__(**kwargs)
        self._title = title
        self._diff_text = diff_text

    def compose(self) -> ComposeResult:
        with Vertical(id="diff-container"):
            yield Label(self._title, classes="modal-title")
            yield RichLog(id="diff-log", wrap=True, highlight=True)

    def on_mount(self) -> None:
        log = self.query_one("#diff-log", RichLog)
        for line in self._diff_text.splitlines() or ["(No diff available)"]:
            log.write(line)

    def action_close(self) -> None:
        self.dismiss(None)
