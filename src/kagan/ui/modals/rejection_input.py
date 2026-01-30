"""Modal for entering rejection feedback."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual import on
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Label, Rule, TextArea

from kagan.constants import MODAL_TITLE_MAX_LENGTH
from kagan.keybindings import REJECTION_INPUT_BINDINGS, to_textual_bindings

if TYPE_CHECKING:
    from textual.app import ComposeResult


class RejectionInputModal(ModalScreen[str | None]):
    """Modal for entering rejection feedback."""

    BINDINGS = to_textual_bindings(REJECTION_INPUT_BINDINGS)

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
                yield Button("Submit", variant="primary", id="submit-btn")
                yield Button("Cancel", id="cancel-btn")

        yield Footer()

    def on_mount(self) -> None:
        """Focus the text area on mount."""
        self.query_one("#feedback-input", TextArea).focus()

    @on(Button.Pressed, "#submit-btn")
    def on_submit_btn(self) -> None:
        self.action_submit()

    @on(Button.Pressed, "#cancel-btn")
    def on_cancel_btn(self) -> None:
        self.action_cancel()

    def action_submit(self) -> None:
        """Submit the feedback."""
        text_area = self.query_one("#feedback-input", TextArea)
        feedback = text_area.text.strip()
        self.dismiss(feedback if feedback else None)

    def action_cancel(self) -> None:
        """Cancel and dismiss without feedback."""
        self.dismiss(None)
