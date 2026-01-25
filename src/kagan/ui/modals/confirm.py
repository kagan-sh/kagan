"""Confirmation modal."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.binding import Binding
from textual.containers import Container
from textual.screen import ModalScreen
from textual.widgets import Label

if TYPE_CHECKING:
    from textual.app import ComposeResult


class ConfirmModal(ModalScreen[bool]):
    """Generic confirmation modal with Yes/No."""

    BINDINGS = [
        Binding("y", "confirm", "Yes"),
        Binding("n", "cancel", "No"),
        Binding("escape", "cancel", "Cancel"),
    ]

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

    def action_confirm(self) -> None:
        self.dismiss(True)

    def action_cancel(self) -> None:
        self.dismiss(False)
