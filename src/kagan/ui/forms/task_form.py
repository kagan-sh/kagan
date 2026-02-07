"""Form field factory for task modals.

Separates form generation logic from modal behavior.
Based on the factory pattern from JiraTUI.
"""

from __future__ import annotations

from enum import Enum, auto
from typing import TYPE_CHECKING

from textual.containers import Horizontal, Vertical
from textual.widgets import Label, Select, Static

from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
from kagan.ui.widgets.base import (
    AcceptanceCriteriaArea,
    AgentBackendSelect,
    DescriptionArea,
    PrioritySelect,
    StatusSelect,
    TaskTypeSelect,
    TitleInput,
)

if TYPE_CHECKING:
    from collections.abc import Sequence

    from textual.app import ComposeResult
    from textual.widget import Widget

    from kagan.core.models.entities import Task


class FormMode(Enum):
    """Form display mode."""

    CREATE = auto()
    VIEW = auto()
    EDIT = auto()


class TaskFormBuilder:
    """Factory for generating task form fields based on mode.

    Separates form generation from modal behavior for cleaner code.
    """

    @staticmethod
    def build_field_selects(
        task: Task | None,
        mode: FormMode,
        agent_options: Sequence[tuple[str, str]] | None = None,
    ) -> ComposeResult:
        """Build the priority/type/agent select row for edit mode.

        Args:
            task: The task being edited/viewed, or None for create.
            mode: Current form mode.
            agent_options: Available agent backend options.

        Yields:
            Widgets for the field select row.
        """
        if mode == FormMode.VIEW:
            return

        current_priority = task.priority if task else TaskPriority.MEDIUM

        current_type = task.task_type if task else TaskType.PAIR

        current_backend = task.agent_backend if task else ""

        with Horizontal(classes="field-row edit-fields", id="edit-fields-row"):
            with Vertical(classes="form-field field-third"):
                yield Label("Priority:", classes="form-label")
                yield PrioritySelect(value=current_priority)

            with Vertical(classes="form-field field-third"):
                yield Label("Type:", classes="form-label")
                # Disable type selector when editing existing task
                is_editing = task is not None
                yield TaskTypeSelect(value=current_type, disabled=is_editing)

            with Vertical(classes="form-field field-third"):
                yield Label("Agent:", classes="form-label")
                opts = agent_options if agent_options else [("Default", "")]
                yield AgentBackendSelect(options=opts, value=current_backend or "")

    @staticmethod
    def build_status_field(
        task: Task | None,
        mode: FormMode,
    ) -> ComposeResult:
        """Build the status select field (only shown in create mode).

        Args:
            task: The task being edited/viewed, or None for create.
            mode: Current form mode.

        Yields:
            Status field widget if in create mode.
        """
        if mode != FormMode.CREATE:
            return

        with Vertical(classes="form-field edit-fields", id="status-field"):
            yield Label("Status:", classes="form-label")
            yield StatusSelect(value=TaskStatus.BACKLOG)

    @staticmethod
    def build_title_field(
        task: Task | None,
        mode: FormMode,
    ) -> ComposeResult:
        """Build title field (view or edit).

        Args:
            task: The task being edited/viewed, or None for create.
            mode: Current form mode.

        Yields:
            Title field widgets.
        """
        title = task.title if task else ""

        # View mode: show static display
        yield Label("Title", classes="section-title view-only", id="title-section-label")
        yield Static(title, classes="task-title view-only", id="title-display", markup=False)

        # Edit mode: show input
        with Vertical(classes="form-field edit-fields", id="title-field"):
            yield TitleInput(value=title)

    @staticmethod
    def build_description_field(
        task: Task | None,
        mode: FormMode,
        editing: bool = False,
    ) -> ComposeResult:
        """Build description field with header.

        Args:
            task: The task being edited/viewed, or None for create.
            mode: Current form mode.
            editing: Whether currently in editing mode.

        Yields:
            Description field widgets.
        """
        description = (task.description if task else "") or "(No description)"

        with Horizontal(classes="description-header"):
            yield Label("Description", classes="section-title")
            yield Static("", classes="header-spacer")
            expand_text = "[F5] Full Editor" if editing else "[f] Expand"
            yield Static(expand_text, classes="expand-hint", id="expand-btn")

        # View mode display
        yield Static(
            description,
            classes="task-description view-only",
            id="description-content",
            markup=False,
        )

        # Edit mode input
        edit_text = task.description if task else ""
        with Vertical(classes="form-field edit-fields", id="description-field"):
            yield DescriptionArea(text=edit_text)

    @staticmethod
    def build_acceptance_criteria_field(
        task: Task | None,
        mode: FormMode,
    ) -> ComposeResult:
        """Build acceptance criteria section.

        Args:
            task: The task being edited/viewed, or None for create.
            mode: Current form mode.

        Yields:
            Acceptance criteria widgets.
        """
        # View mode: show existing criteria
        if task and task.acceptance_criteria:
            with Vertical(classes="acceptance-criteria-section view-only", id="ac-section"):
                yield Label("Acceptance Criteria", classes="section-title")
                for criterion in task.acceptance_criteria:
                    yield Static(f"  - {criterion}", classes="ac-item")

        # Edit mode: show textarea
        criteria = task.acceptance_criteria if task else []
        with Vertical(classes="form-field edit-fields", id="ac-field"):
            yield Label("Acceptance Criteria (one per line):", classes="form-label")
            yield AcceptanceCriteriaArea(criteria=criteria)

    @staticmethod
    def get_form_values(container: Widget) -> dict[str, object]:
        """Extract current form values from widgets.

        Args:
            container: The container widget holding form fields.

        Returns:
            Dictionary of field name to value.
        """
        from textual.widgets import Input, TextArea

        values: dict[str, object] = {}

        try:
            title_input = container.query_one("#title-input", Input)
            values["title"] = title_input.value.strip()
        except Exception:
            pass

        try:
            desc_input = container.query_one("#description-input", TextArea)
            values["description"] = desc_input.text
        except Exception:
            pass

        try:
            priority_select: Select[int] = container.query_one("#priority-select", Select)
            if priority_select.value is not Select.BLANK:
                from typing import cast

                values["priority"] = TaskPriority(cast("int", priority_select.value))
        except Exception:
            pass

        try:
            type_select: Select[str] = container.query_one("#type-select", Select)
            if type_select.value is not Select.BLANK:
                values["task_type"] = TaskType(str(type_select.value))
        except Exception:
            pass

        try:
            agent_select: Select[str] = container.query_one("#agent-backend-select", Select)
            if agent_select.value is not Select.BLANK:
                values["agent_backend"] = str(agent_select.value) or None
            else:
                values["agent_backend"] = None
        except Exception:
            pass

        try:
            status_select: Select[str] = container.query_one("#status-select", Select)
            if status_select.value is not Select.BLANK:
                values["status"] = TaskStatus(str(status_select.value))
        except Exception:
            pass

        try:
            ac_input = container.query_one("#ac-input", AcceptanceCriteriaArea)
            values["acceptance_criteria"] = ac_input.get_criteria()
        except Exception:
            pass

        return values

    @staticmethod
    def reset_form_to_task(container: Widget, task: Task) -> None:
        """Reset form fields to match task values.

        Args:
            container: The container widget holding form fields.
            task: The task to reset values from.
        """
        from textual.widgets import Input, Select, TextArea

        from kagan.ui.utils import safe_query_one

        if title_input := safe_query_one(container, "#title-input", Input):
            title_input.value = task.title

        if desc_input := safe_query_one(container, "#description-input", TextArea):
            desc_input.text = task.description or ""

        if priority_select := safe_query_one(container, "#priority-select", Select):
            priority_select.value = task.priority.value

        if type_select := safe_query_one(container, "#type-select", Select):
            type_select.value = task.task_type.value

        if agent_select := safe_query_one(container, "#agent-backend-select", Select):
            agent_select.value = task.agent_backend or ""

        if ac_input := safe_query_one(container, "#ac-input", TextArea):
            ac_text = "\n".join(task.acceptance_criteria) if task.acceptance_criteria else ""
            ac_input.text = ac_text
