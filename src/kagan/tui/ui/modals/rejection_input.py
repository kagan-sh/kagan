"""Modal for entering rejection feedback.

Returns a tuple of (feedback_text, action) where action is one of:
- "return": Move task to IN_PROGRESS (manual restart)
- "backlog": Move task to BACKLOG
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual import on
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Label, Rule, TextArea

from kagan.core.constants import MODAL_TITLE_MAX_LENGTH
from kagan.tui.keybindings import REJECTION_INPUT_BINDINGS

if TYPE_CHECKING:
    from textual.app import ComposeResult


class RejectionInputModal(ModalScreen[tuple[str, str] | None]):
    """Modal for entering rejection feedback.

    Returns:
        tuple[str, str] | None: A tuple of (feedback_text, action) where action
            is "return" or "backlog", or None if dismissed.
    """

    BINDINGS = REJECTION_INPUT_BINDINGS

    def __init__(self, task_title: str, **kwargs) -> None:
        super().__init__(**kwargs)
        self._task_title = task_title

    def compose(self) -> ComposeResult:
        with Vertical(id="rejection-input-container"):
            yield Label("Rejection Feedback", classes="modal-title")
            yield Label(f"Task: {self._task_title[:MODAL_TITLE_MAX_LENGTH]}", classes="task-label")
            yield Rule()
            yield Label("What needs to be fixed?", classes="prompt-label")
            yield TextArea(id="feedback-input")
            yield Rule()
            with Horizontal(classes="button-row"):
                yield Button("Back to In Progress", variant="primary", id="return-btn")
                yield Button("Backlog", variant="error", id="backlog-btn")

        yield Footer(show_command_palette=False)

    def on_mount(self) -> None:
        """Focus the text area on mount."""
        self.query_one("#feedback-input", TextArea).focus()

    @on(Button.Pressed, "#return-btn")
    def on_return_btn(self) -> None:
        self.action_send_back()

    @on(Button.Pressed, "#backlog-btn")
    def on_backlog_btn(self) -> None:
        self.action_backlog()

    def _get_feedback(self) -> str:
        """Get the feedback text from the input area."""
        text_area = self.query_one("#feedback-input", TextArea)
        return text_area.text.strip()

    def action_send_back(self) -> None:
        """Submit feedback and send back to IN_PROGRESS (manual restart)."""
        feedback = self._get_feedback()
        self.dismiss((feedback, "return"))

    def action_backlog(self) -> None:
        """Send the task to BACKLOG."""
        feedback = self._get_feedback()
        self.dismiss((feedback, "backlog"))
