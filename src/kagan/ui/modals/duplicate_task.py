"""Modal for duplicating a task with selectable fields."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual import on
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Checkbox, Footer, Input, Label, Rule, Static

from kagan.keybindings import DUPLICATE_TASK_BINDINGS

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.core.models.entities import Task


class DuplicateTaskModal(ModalScreen[dict[str, object] | None]):
    """Modal for duplicating a task with selectable fields to copy."""

    BINDINGS = DUPLICATE_TASK_BINDINGS

    def __init__(self, source_task: Task, **kwargs) -> None:
        super().__init__(**kwargs)
        self.source = source_task

    def compose(self) -> ComposeResult:
        with Vertical(id="duplicate-container"):
            yield Label("Duplicate Task", classes="modal-title")
            yield Static(f"Based on #{self.source.short_id}", classes="source-ref")
            yield Rule()

            with Vertical(classes="form-field"):
                yield Label("Title:", classes="form-label")
                yield Input(value=self.source.title, id="title-input")

            yield Rule()
            yield Label("Copy fields:", classes="section-title")

            with Vertical(classes="checkbox-group"):
                yield Checkbox("Description", value=True, id="copy-description")
                yield Checkbox("Acceptance Criteria", value=False, id="copy-criteria")
                yield Checkbox("Priority", value=False, id="copy-priority")
                yield Checkbox("Task Type", value=False, id="copy-type")
                yield Checkbox("Agent Backend", value=False, id="copy-agent")

            yield Rule()
            with Horizontal(classes="button-row"):
                yield Button("[Enter] Create", variant="primary", id="create-btn")
                yield Button("[Esc] Cancel", id="cancel-btn")

        yield Footer(show_command_palette=False)

    def on_mount(self) -> None:
        """Focus the title input when modal opens."""
        self.query_one("#title-input", Input).focus()

    @on(Button.Pressed, "#create-btn")
    def on_create_btn(self) -> None:
        self.action_create()

    @on(Button.Pressed, "#cancel-btn")
    def on_cancel_btn(self) -> None:
        self.action_cancel()

    def action_create(self) -> None:
        """Create the duplicate task with selected fields."""
        title = self.query_one("#title-input", Input).value.strip()
        if not title:
            self.notify("Title is required", severity="error")
            return

        description = ""
        if self.query_one("#copy-description", Checkbox).value:
            description = self.source.description

        acceptance_criteria: list[str] = []
        if self.query_one("#copy-criteria", Checkbox).value:
            acceptance_criteria = list(self.source.acceptance_criteria)

        priority = None
        if self.query_one("#copy-priority", Checkbox).value:
            priority = self.source.priority

        task_type = None
        if self.query_one("#copy-type", Checkbox).value:
            task_type = self.source.task_type

        agent_backend = None
        if self.query_one("#copy-agent", Checkbox).value:
            agent_backend = self.source.agent_backend

        kwargs: dict[str, object] = {
            "title": title,
            "description": description,
            "acceptance_criteria": acceptance_criteria,
        }
        if priority is not None:
            kwargs["priority"] = priority
        if task_type is not None:
            kwargs["task_type"] = task_type
        if agent_backend is not None:
            kwargs["agent_backend"] = agent_backend

        self.dismiss(kwargs)

    def action_cancel(self) -> None:
        """Cancel and close the modal."""
        self.dismiss(None)
