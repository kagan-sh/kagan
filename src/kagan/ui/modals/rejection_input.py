"""Modal for entering rejection feedback.

Returns a tuple of (feedback_text, action) where action is one of:
- "retry": Move ticket to IN_PROGRESS and auto-restart agent
- "stage": Move ticket to IN_PROGRESS and keep paused
- None: Shelve/cancel - move ticket to BACKLOG
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual import on
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Label, Rule, TextArea

from kagan.constants import MODAL_TITLE_MAX_LENGTH
from kagan.keybindings import REJECTION_INPUT_BINDINGS

if TYPE_CHECKING:
    from textual.app import ComposeResult


class RejectionInputModal(ModalScreen[tuple[str, str] | None]):
    """Modal for entering rejection feedback.

    Returns:
        tuple[str, str] | None: A tuple of (feedback_text, action) where action
            is "retry" or "stage", or None if shelved/cancelled.
    """

    BINDINGS = REJECTION_INPUT_BINDINGS

    def __init__(self, ticket_title: str, **kwargs) -> None:
        super().__init__(**kwargs)
        self._ticket_title = ticket_title

    def compose(self) -> ComposeResult:
        with Vertical(id="rejection-input-container"):
            yield Label("Rejection Feedback", classes="modal-title")
            yield Label(
                f"Ticket: {self._ticket_title[:MODAL_TITLE_MAX_LENGTH]}", classes="ticket-label"
            )
            yield Rule()
            yield Label("What needs to be fixed?", classes="prompt-label")
            yield TextArea(id="feedback-input")
            yield Rule()
            with Horizontal(classes="button-row"):
                yield Button("Retry", variant="primary", id="retry-btn")
                yield Button("Stage", variant="default", id="stage-btn")
                yield Button("Shelve", variant="error", id="shelve-btn")

        yield Footer()

    def on_mount(self) -> None:
        """Focus the text area on mount."""
        self.query_one("#feedback-input", TextArea).focus()

    @on(Button.Pressed, "#retry-btn")
    def on_retry_btn(self) -> None:
        self.action_retry()

    @on(Button.Pressed, "#stage-btn")
    def on_stage_btn(self) -> None:
        self.action_stage()

    @on(Button.Pressed, "#shelve-btn")
    def on_shelve_btn(self) -> None:
        self.action_shelve()

    def _get_feedback(self) -> str:
        """Get the feedback text from the input area."""
        text_area = self.query_one("#feedback-input", TextArea)
        return text_area.text.strip()

    def action_retry(self) -> None:
        """Submit feedback and retry - move to IN_PROGRESS and auto-restart agent."""
        feedback = self._get_feedback()
        self.dismiss((feedback, "retry"))

    def action_stage(self) -> None:
        """Submit feedback and stage - move to IN_PROGRESS but keep paused."""
        feedback = self._get_feedback()
        self.dismiss((feedback, "stage"))

    def action_shelve(self) -> None:
        """Shelve the ticket - move to BACKLOG."""
        self.dismiss(None)
