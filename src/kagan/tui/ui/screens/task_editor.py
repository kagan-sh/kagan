"""Task editor screen for refining proposed tasks from planner."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from textual import on
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, Select, TabbedContent, TabPane, TextArea

from kagan.core.adapters.db.schema import Task
from kagan.core.models.enums import VALID_PAIR_BACKENDS, TaskPriority, TaskType
from kagan.tui.keybindings import TASK_EDITOR_BINDINGS
from kagan.tui.ui.widgets.base import BaseBranchInput, PairTerminalBackendSelect

if TYPE_CHECKING:
    from textual.app import ComposeResult
    from textual.widget import Widget


PRIORITY_OPTIONS = [
    ("Low", TaskPriority.LOW.value),
    ("Medium", TaskPriority.MEDIUM.value),
    ("High", TaskPriority.HIGH.value),
]
TASK_TYPE_OPTIONS = [
    ("AUTO - AI completes autonomously", TaskType.AUTO.value),
    ("PAIR - Human collaboration needed", TaskType.PAIR.value),
]


@dataclass(frozen=True, slots=True)
class TaskFieldIds:
    title: str
    description: str
    priority: str
    task_type: str
    terminal_backend: str
    base_branch: str
    acceptance_criteria: str

    @classmethod
    def for_index(cls, index: int) -> TaskFieldIds:
        suffix = str(index)
        return cls(
            title=f"#title-{suffix}",
            description=f"#description-{suffix}",
            priority=f"#priority-{suffix}",
            task_type=f"#type-{suffix}",
            terminal_backend=f"#terminal-backend-{suffix}",
            base_branch=f"#base-branch-{suffix}",
            acceptance_criteria=f"#ac-{suffix}",
        )


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
                            field_ids = TaskFieldIds.for_index(i)
                            yield from self._build_task_widgets(task, field_ids)
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
            field_ids = TaskFieldIds.for_index(i)
            edited_tasks.append(self._collect_task_from_fields(original, field_ids))

        return edited_tasks

    def _build_task_widgets(self, task: Task, field_ids: TaskFieldIds) -> list[Widget]:
        ac_text = "\n".join(task.acceptance_criteria) if task.acceptance_criteria else ""
        terminal_backend = self._normalize_terminal_backend(getattr(task, "terminal_backend", None))
        return [
            Input(
                value=task.title,
                placeholder="Title",
                id=field_ids.title.removeprefix("#"),
                classes="task-input",
            ),
            TextArea(
                text=task.description or "",
                id=field_ids.description.removeprefix("#"),
                classes="task-textarea",
            ),
            Select(
                options=PRIORITY_OPTIONS,
                value=task.priority.value,
                id=field_ids.priority.removeprefix("#"),
                classes="task-select",
            ),
            Select(
                options=TASK_TYPE_OPTIONS,
                value=task.task_type.value,
                id=field_ids.task_type.removeprefix("#"),
                classes="task-select",
            ),
            Label("PAIR Terminal Backend:", classes="task-label"),
            PairTerminalBackendSelect(
                value=terminal_backend,
                disabled=task.task_type != TaskType.PAIR,
                widget_id=field_ids.terminal_backend.removeprefix("#"),
                classes="task-select",
            ),
            Label("Base Branch:", classes="task-label"),
            BaseBranchInput(
                value=task.base_branch or "",
                widget_id=field_ids.base_branch.removeprefix("#"),
            ),
            Label("Acceptance Criteria (one per line):", classes="task-label"),
            TextArea(
                text=ac_text,
                id=field_ids.acceptance_criteria.removeprefix("#"),
                classes="task-textarea",
            ),
        ]

    def _collect_task_from_fields(self, original: Task, field_ids: TaskFieldIds) -> Task:
        title_input = self.query_one(field_ids.title, Input)
        description_input = self.query_one(field_ids.description, TextArea)
        priority_select: Select[int] = self.query_one(field_ids.priority, Select)
        type_select: Select[str] = self.query_one(field_ids.task_type, Select)
        terminal_backend_select: Select[str] = self.query_one(field_ids.terminal_backend, Select)
        base_branch_input = self.query_one(field_ids.base_branch, BaseBranchInput)
        ac_input = self.query_one(field_ids.acceptance_criteria, TextArea)

        ac_lines = ac_input.text.strip().split("\n") if ac_input.text.strip() else []
        acceptance_criteria = [line.strip() for line in ac_lines if line.strip()]

        title = title_input.value.strip() or original.title
        description = description_input.text or original.description

        priority_value = priority_select.value
        if priority_value is Select.BLANK:
            priority = original.priority
        else:
            if not isinstance(priority_value, int):
                msg = f"Unexpected priority value type: {type(priority_value).__name__}"
                raise ValueError(msg)
            priority = TaskPriority(priority_value)

        type_value = type_select.value
        if type_value is Select.BLANK:
            task_type = original.task_type
        else:
            if not isinstance(type_value, str):
                msg = f"Unexpected task type value type: {type(type_value).__name__}"
                raise ValueError(msg)
            task_type = TaskType(type_value)

        terminal_backend_value = terminal_backend_select.value
        terminal_backend = self._normalize_terminal_backend(terminal_backend_value)
        if task_type == TaskType.AUTO:
            terminal_backend = None

        base_branch = base_branch_input.value.strip() or None

        task_payload = {
            "id": original.id,
            "project_id": original.project_id,
            "title": title,
            "description": description,
            "status": original.status,
            "priority": priority,
            "task_type": task_type,
            "agent_backend": original.agent_backend,
            "parent_id": original.parent_id,
            "acceptance_criteria": acceptance_criteria,
            "base_branch": base_branch,
            "created_at": original.created_at,
            "updated_at": original.updated_at,
        }
        if "terminal_backend" in Task.model_fields:
            task_payload["terminal_backend"] = terminal_backend
        return Task(**task_payload)

    def _normalize_terminal_backend(self, value: object) -> str:
        if value is Select.BLANK or not isinstance(value, str):
            return "tmux"
        return value if value in VALID_PAIR_BACKENDS else "tmux"

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
