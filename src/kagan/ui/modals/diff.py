"""Modal for viewing git diffs."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Footer, Label, RichLog

from kagan.keybindings import DIFF_BINDINGS, to_textual_bindings
from kagan.ui.utils.clipboard import copy_with_notification

if TYPE_CHECKING:
    from textual.app import ComposeResult


class DiffModal(ModalScreen[None]):
    """Modal for showing a ticket diff."""

    BINDINGS = to_textual_bindings(DIFF_BINDINGS)

    def __init__(self, title: str, diff_text: str, **kwargs) -> None:
        super().__init__(**kwargs)
        self._title = title
        self._diff_text = diff_text

    def compose(self) -> ComposeResult:
        with Vertical(id="diff-container"):
            yield Label(self._title, classes="modal-title")
            yield RichLog(id="diff-log", wrap=True, highlight=True)
        yield Footer()

    def on_mount(self) -> None:
        log = self.query_one("#diff-log", RichLog)
        for line in self._diff_text.splitlines() or ["(No diff available)"]:
            log.write(line)

    def action_close(self) -> None:
        self.dismiss(None)

    def action_copy(self) -> None:
        """Copy diff content to clipboard."""
        copy_with_notification(self.app, self._diff_text, "Diff")
