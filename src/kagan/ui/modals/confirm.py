"""Confirmation modal."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Container
from textual.screen import ModalScreen
from textual.widgets import Footer, Label

from kagan.keybindings import CONFIRM_BINDINGS, to_textual_bindings

if TYPE_CHECKING:
    from textual.app import ComposeResult


class ConfirmModal(ModalScreen[bool]):
    """Generic confirmation modal with Yes/No."""

    BINDINGS = to_textual_bindings(CONFIRM_BINDINGS)

    def __init__(self, title: str = "Confirm?", message: str = "", **kwargs) -> None:
        super().__init__(**kwargs)
        self._title = title
        self._message = message

    def compose(self) -> ComposeResult:
        with Container():
            yield Label(self._title, classes="confirm-title")
            if self._message:
                yield Label(self._message, classes="confirm-message")
            yield Label("Press Y to confirm, N to cancel", classes="confirm-hint")
        yield Footer()

    def action_confirm(self) -> None:
        self.dismiss(True)

    def action_cancel(self) -> None:
        self.dismiss(False)
