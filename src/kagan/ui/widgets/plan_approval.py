"""Inline plan approval widget for the planner screen."""

from __future__ import annotations

from typing import TYPE_CHECKING, ClassVar

from textual.binding import Binding, BindingType
from textual.containers import Horizontal, Vertical, VerticalGroup
from textual.message import Message
from textual.widgets import Button, Static

from kagan.core.models.enums import TaskType

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.core.models.entities import Task


class PlanApprovalWidget(VerticalGroup):
    """Inline widget for reviewing and approving generated plan tasks."""

    DEFAULT_CLASSES = "plan-approval"

    BINDINGS: ClassVar[list[BindingType]] = [
        Binding("a", "approve", "Approve"),
        Binding("e", "edit", "Edit"),
        Binding("d", "dismiss", "Dismiss"),
        Binding("escape", "dismiss", "Dismiss"),
    ]

    class Approved(Message):
        """Message posted when the plan is approved."""

        def __init__(self, tasks: list[Task]) -> None:
            super().__init__()
            self.tasks = tasks

    class EditRequested(Message):
        """Message posted when user wants to edit tasks."""

        def __init__(self, tasks: list[Task]) -> None:
            super().__init__()
            self.tasks = tasks

    class Dismissed(Message):
        """Message posted when the plan is dismissed."""

        pass

    def __init__(
        self,
        tasks: list[Task],
        *,
        id: str | None = None,
        classes: str | None = None,
    ) -> None:
        super().__init__(id=id, classes=classes)
        self._tasks = tasks
        self._selected_index = 0

    def compose(self) -> ComposeResult:
        n = len(self._tasks)
        yield Static(f"📋 Generated Plan ({n} task{'s' if n != 1 else ''})", classes="plan-header")
        yield Static("Use ↑/↓ to navigate, Enter to preview", classes="plan-hint")

        with Vertical(id="task-list"):
            for i, task in enumerate(self._tasks):
                yield self._make_task_row(task, i)

        with Horizontal(id="approval-buttons"):
            yield Button("[a] Approve", id="btn-approve", variant="success")
            yield Button("[e] Edit", id="btn-edit", variant="warning")
            yield Button("[d] Dismiss", id="btn-dismiss", variant="error")

    def _make_task_row(self, task: Task, index: int) -> Static:
        """Create a row displaying a single task."""
        type_badge = "⚡ AUTO" if task.task_type == TaskType.AUTO else "👤 PAIR"
        priority_label = task.priority.label.upper()
        title = task.title
        if len(title) > 60:
            title = title[:57] + "..."

        row_text = f"  {type_badge}  {title}  [{priority_label}]"
        classes = f"task-row priority-{task.priority.css_class}"
        if index == self._selected_index:
            classes += " selected"
        return Static(row_text, classes=classes, id=f"task-row-{index}")

    def on_mount(self) -> None:
        """Focus self and update selection."""
        self._update_selection()
        self.focus()

    def _update_selection(self) -> None:
        """Update visual selection state."""
        for i in range(len(self._tasks)):
            row = self.query_one(f"#task-row-{i}", Static)
            if i == self._selected_index:
                row.add_class("selected")
            else:
                row.remove_class("selected")

    def on_key(self, event) -> None:
        """Handle keyboard navigation."""
        if event.key in ("up", "k"):
            if self._selected_index > 0:
                self._selected_index -= 1
                self._update_selection()
            event.stop()
        elif event.key in ("down", "j"):
            if self._selected_index < len(self._tasks) - 1:
                self._selected_index += 1
                self._update_selection()
            event.stop()
        elif event.key == "enter":
            self._show_preview()
            event.stop()

    def _show_preview(self) -> None:
        """Show preview of the selected task."""
        if 0 <= self._selected_index < len(self._tasks):
            task = self._tasks[self._selected_index]

            type_str = (
                "AUTO (AI autonomous)"
                if task.task_type == TaskType.AUTO
                else "PAIR (human collaboration)"
            )
            ac_text = (
                "\n".join(f"  • {c}" for c in task.acceptance_criteria)
                if task.acceptance_criteria
                else "  (none)"
            )

            preview = f"""**{task.title}**

**Type:** {type_str}
**Priority:** {task.priority.label}

**Description:**
{task.description or "(no description)"}

**Acceptance Criteria:**
{ac_text}
"""
            self.notify(preview, title=f"Task {self._selected_index + 1} Preview", timeout=10)

    def action_approve(self) -> None:
        """Approve the plan and post Approved message."""
        self.post_message(self.Approved(self._tasks))
        self.remove()

    def action_edit(self) -> None:
        """Request to edit tasks before approval."""
        self.post_message(self.EditRequested(self._tasks))
        self.remove()

    def action_dismiss(self) -> None:
        """Dismiss the plan and post Dismissed message."""
        self.post_message(self.Dismissed())
        self.remove()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        event.stop()
        if event.button.id == "btn-approve":
            self.action_approve()
        elif event.button.id == "btn-edit":
            self.action_edit()
        elif event.button.id == "btn-dismiss":
            self.action_dismiss()
