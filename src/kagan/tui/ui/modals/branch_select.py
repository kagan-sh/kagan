"""Base-branch selection modal shared by branch configuration flows."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from textual import on
from textual.containers import Horizontal, Vertical
from textual.message import Message
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, Select, Static

if TYPE_CHECKING:
    from textual.app import ComposeResult


_CUSTOM_OPTION = "__custom__"


_SELECT_BLANK: Any = Select.BLANK


class BaseBranchModal(ModalScreen[str | None]):
    """Prompt the user to choose or enter a base branch."""

    BINDINGS = [
        ("escape", "cancel", "Cancel"),
    ]

    @dataclass
    class Submitted(Message):
        """Message emitted when a branch value is submitted."""

        branch: str | None

    def __init__(
        self,
        *,
        branches: list[str] | None = None,
        current_value: str = "",
        title: str = "Set Base Branch",
        description: str = "Select a branch or enter a custom name:",
        **kwargs,
    ) -> None:
        """Initialize modal copy and available branch choices."""
        super().__init__(**kwargs)
        self._branches = branches or []
        self._current_value = current_value
        self._title = title
        self._description = description

    def compose(self) -> ComposeResult:
        """Build modal layout with branch select and optional custom input."""
        with Vertical(id="branch-modal-container"):
            yield Static(self._title, id="branch-modal-title")
            yield Label(self._description, id="branch-modal-description")

            options: list[tuple[str, str]] = []

            if self._current_value and self._current_value not in self._branches:
                options.append((f"{self._current_value} (current)", self._current_value))

            for branch in self._branches:
                label = f"{branch} (current)" if branch == self._current_value else branch
                options.append((label, branch))

            options.append(("+ Custom branch...", _CUSTOM_OPTION))

            yield Select[str](
                options,
                value=self._current_value if self._current_value else _SELECT_BLANK,
                id="branch-select",
                prompt="Select a branch...",
            )

            yield Input(
                placeholder="Enter branch name (e.g. main, develop)",
                id="branch-custom-input",
                classes="hidden",
            )

            with Horizontal(id="branch-modal-buttons"):
                yield Button("Cancel", variant="default", id="cancel-btn")
                yield Button("Set Branch", variant="primary", id="submit-btn")

    def on_mount(self) -> None:
        """Focus branch selector when the modal is mounted."""
        self.query_one("#branch-select", Select).focus()

    @on(Select.Changed, "#branch-select")
    def on_select_changed(self, event: Select.Changed) -> None:
        """Show custom input only when the custom option is selected."""
        custom_input = self.query_one("#branch-custom-input", Input)
        if event.value == _CUSTOM_OPTION:
            custom_input.remove_class("hidden")
            custom_input.focus()
        else:
            custom_input.add_class("hidden")

    @on(Input.Submitted, "#branch-custom-input")
    def on_custom_input_submitted(self) -> None:
        """Submit the branch when Enter is pressed in custom input."""
        self._submit_branch()

    @on(Button.Pressed, "#cancel-btn")
    def on_cancel(self) -> None:
        """Dismiss modal without selecting a branch."""
        self.dismiss(None)

    @on(Button.Pressed, "#submit-btn")
    def on_submit(self) -> None:
        """Submit currently selected or custom branch value."""
        self._submit_branch()

    def action_cancel(self) -> None:
        """Handle escape binding by dismissing the modal."""
        self.dismiss(None)

    def _submit_branch(self) -> None:
        select_widget = self.query_one("#branch-select", Select)
        custom_input = self.query_one("#branch-custom-input", Input)

        if select_widget.value == _CUSTOM_OPTION:
            value = custom_input.value.strip()
        elif select_widget.value == Select.BLANK:
            value = ""
        else:
            value = str(select_widget.value)

        self.dismiss(value or None)
