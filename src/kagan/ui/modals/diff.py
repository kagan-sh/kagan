"""Modal for viewing git diffs."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Footer, Label, RichLog

from kagan.keybindings import DIFF_BINDINGS
from kagan.ui.utils.clipboard import copy_with_notification

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.database.models import Ticket


class DiffModal(ModalScreen[str | None]):
    """Modal for showing a ticket diff.

    Returns:
        str | None:
            - "approve" if user pressed 'a'
            - "reject" if user pressed 'r'
            - None if user just closed the modal
    """

    BINDINGS = DIFF_BINDINGS

    def __init__(self, title: str, diff_text: str, ticket: Ticket | None = None, **kwargs) -> None:
        super().__init__(**kwargs)
        self._title = title
        self._diff_text = diff_text
        self._ticket = ticket

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
        """Close the modal without any action."""
        self.dismiss(None)

    def action_approve(self) -> None:
        """Approve and dismiss the modal."""
        self.dismiss("approve")

    def action_reject(self) -> None:
        """Reject and dismiss the modal."""
        self.dismiss("reject")

    def action_copy(self) -> None:
        """Copy diff content to clipboard."""
        copy_with_notification(self.app, self._diff_text, "Diff")
