"""Task editor screen for refining proposed tasks from planner."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

from textual import on
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, Select, TabbedContent, TabPane, TextArea

from kagan.core.models.entities import Task
from kagan.core.models.enums import TaskPriority, TaskType
from kagan.keybindings import TASK_EDITOR_BINDINGS
from kagan.ui.widgets.base import BaseBranchInput, PairTerminalBackendSelect

if TYPE_CHECKING:
    from textual.app import ComposeResult


VALID_PAIR_LAUNCHERS = {"tmux", "vscode", "cursor"}


class TaskEditorScreen(ModalScreen[list[Task] | None]):
    """Edit proposed tasks before approval.

    Returns:
        list[Task]: Edited tasks
        None: User cancelled
    """

    BINDINGS = TASK_EDITOR_BINDINGS

    def __init__(self, tasks: list[Task]) -> None:
        super().__init__()
        self._tasks = list(tasks)

    def compose(self) -> ComposeResult:
        with Vertical(id="task-editor-container"):
            with TabbedContent(id="task-tabs"):
                for i, task in enumerate(self._tasks, 1):
                    with TabPane(f"Task {i}", id=f"task-{i}"):
                        with Vertical(classes="task-form"):
                            yield Input(
                                value=task.title,
                                placeholder="Title",
                                id=f"title-{i}",
                                classes="task-input",
                            )
                            yield TextArea(
                                text=task.description or "",
                                id=f"description-{i}",
                                classes="task-textarea",
                            )
                            yield Select(
                                options=[
                                    ("Low", TaskPriority.LOW.value),
                                    ("Medium", TaskPriority.MEDIUM.value),
                                    ("High", TaskPriority.HIGH.value),
                                ],
                                value=task.priority.value,
                                id=f"priority-{i}",
                                classes="task-select",
                            )
                            yield Select(
                                options=[
                                    ("AUTO - AI completes autonomously", TaskType.AUTO.value),
                                    ("PAIR - Human collaboration needed", TaskType.PAIR.value),
                                ],
                                value=task.task_type.value,
                                id=f"type-{i}",
                                classes="task-select",
                            )
                            yield Label("PAIR Terminal Backend:", classes="task-label")
                            pair_terminal_backend = getattr(task, "terminal_backend", None)
                            terminal_backend: str = (
                                pair_terminal_backend
                                if isinstance(pair_terminal_backend, str)
                                and pair_terminal_backend in VALID_PAIR_LAUNCHERS
                                else "tmux"
                            )
                            yield PairTerminalBackendSelect(
                                value=terminal_backend,
                                disabled=task.task_type != TaskType.PAIR,
                                widget_id=f"terminal-backend-{i}",
                                classes="task-select",
                            )
                            yield Label("Base Branch:", classes="task-label")
                            yield BaseBranchInput(
                                value=task.base_branch or "",
                                widget_id=f"base-branch-{i}",
                            )
                            yield Label("Acceptance Criteria (one per line):", classes="task-label")
                            ac_text = (
                                "\n".join(task.acceptance_criteria)
                                if task.acceptance_criteria
                                else ""
                            )
                            yield TextArea(
                                text=ac_text,
                                id=f"ac-{i}",
                                classes="task-textarea",
                            )
            yield Button("Finish Editing", id="finish-btn", variant="primary")

    def on_mount(self) -> None:
        """Focus the first input field."""
        if self._tasks:
            first_input = self.query_one("#title-1", Input)
            first_input.focus()

    def _collect_edited_tasks(self) -> list[Task]:
        """Collect all edited tasks from the form fields."""
        edited_tasks: list[Task] = []

        for i, original in enumerate(self._tasks, 1):
            title_input = self.query_one(f"#title-{i}", Input)
            description_input = self.query_one(f"#description-{i}", TextArea)
            priority_select: Select[int] = self.query_one(f"#priority-{i}", Select)
            type_select: Select[str] = self.query_one(f"#type-{i}", Select)
            terminal_backend_select: Select[str] = self.query_one(f"#terminal-backend-{i}", Select)
            base_branch_input = self.query_one(f"#base-branch-{i}", BaseBranchInput)
            ac_input = self.query_one(f"#ac-{i}", TextArea)

            ac_lines = ac_input.text.strip().split("\n") if ac_input.text.strip() else []
            acceptance_criteria = [line.strip() for line in ac_lines if line.strip()]

            title = title_input.value.strip() or original.title
            description = description_input.text or original.description

            priority_value = priority_select.value
            if priority_value is Select.BLANK:
                priority = original.priority
            else:
                priority = TaskPriority(cast("int", priority_value))

            type_value = type_select.value
            if type_value is Select.BLANK:
                task_type = original.task_type
            else:
                task_type = TaskType(cast("str", type_value))

            terminal_backend_value = terminal_backend_select.value
            if terminal_backend_value is Select.BLANK:
                terminal_backend = "tmux"
            else:
                selected_backend = str(terminal_backend_value)
                terminal_backend = (
                    selected_backend if selected_backend in VALID_PAIR_LAUNCHERS else "tmux"
                )
            if task_type == TaskType.AUTO:
                terminal_backend = None

            base_branch = base_branch_input.value.strip() or None

            if "terminal_backend" in Task.model_fields:
                edited_tasks.append(
                    Task(
                        id=original.id,
                        project_id=original.project_id,
                        title=title,
                        description=description,
                        status=original.status,
                        priority=priority,
                        task_type=task_type,
                        terminal_backend=terminal_backend,
                        assigned_hat=original.assigned_hat,
                        agent_backend=original.agent_backend,
                        parent_id=original.parent_id,
                        acceptance_criteria=acceptance_criteria,
                        base_branch=base_branch,
                        created_at=original.created_at,
                        updated_at=original.updated_at,
                    )
                )
            else:
                edited_tasks.append(
                    Task(
                        id=original.id,
                        project_id=original.project_id,
                        title=title,
                        description=description,
                        status=original.status,
                        priority=priority,
                        task_type=task_type,
                        assigned_hat=original.assigned_hat,
                        agent_backend=original.agent_backend,
                        parent_id=original.parent_id,
                        acceptance_criteria=acceptance_criteria,
                        base_branch=base_branch,
                        created_at=original.created_at,
                        updated_at=original.updated_at,
                    )
                )

        return edited_tasks

    @on(Select.Changed)
    def on_type_changed(self, event: Select.Changed) -> None:
        if event.select.id is None or not event.select.id.startswith("type-"):
            return
        suffix = event.select.id.removeprefix("type-")
        terminal_select = self.query_one(f"#terminal-backend-{suffix}", Select)
        value = event.select.value
        is_pair = value is not Select.BLANK and str(value) == TaskType.PAIR.value
        terminal_select.disabled = not is_pair

    def action_finish(self) -> None:
        """Finish editing and return the edited tasks."""
        edited_tasks = self._collect_edited_tasks()
        self.dismiss(edited_tasks)

    def action_cancel(self) -> None:
        """Cancel and dismiss without changes."""
        self.dismiss(None)

    @on(Button.Pressed, "#finish-btn")
    def on_finish(self) -> None:
        """Handle finish button press."""
        self.action_finish()
