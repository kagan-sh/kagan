"""Reusable base widget classes for forms and modals."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.widgets import Input, Select, TextArea

from kagan.core.constants import PRIORITY_LABELS
from kagan.core.models.enums import PairTerminalBackend, TaskPriority, TaskStatus, TaskType

if TYPE_CHECKING:
    from collections.abc import Sequence


class TitleInput(Input):
    """Reusable title input with consistent styling and validation."""

    DEFAULT_PLACEHOLDER = "Enter task title..."
    DEFAULT_MAX_LENGTH = 200

    def __init__(
        self,
        value: str = "",
        *,
        placeholder: str | None = None,
        max_length: int | None = None,
        widget_id: str = "title-input",
        **kwargs,
    ) -> None:
        super().__init__(
            value=value,
            placeholder=placeholder or self.DEFAULT_PLACEHOLDER,
            max_length=max_length or self.DEFAULT_MAX_LENGTH,
            id=widget_id,
            **kwargs,
        )


class DescriptionArea(TextArea):
    """Reusable description textarea with consistent styling."""

    DEFAULT_PLACEHOLDER = "Enter description..."

    def __init__(
        self,
        text: str = "",
        *,
        show_line_numbers: bool = True,
        widget_id: str = "description-input",
        **kwargs,
    ) -> None:
        super().__init__(
            text=text,
            show_line_numbers=show_line_numbers,
            id=widget_id,
            **kwargs,
        )


class AcceptanceCriteriaArea(TextArea):
    """TextArea for acceptance criteria (one per line)."""

    def __init__(
        self,
        criteria: list[str] | None = None,
        *,
        widget_id: str = "ac-input",
        **kwargs,
    ) -> None:
        text = "\n".join(criteria) if criteria else ""
        super().__init__(
            text=text,
            id=widget_id,
            **kwargs,
        )

    def get_criteria(self) -> list[str]:
        """Parse and return acceptance criteria as list."""
        lines = self.text.strip().split("\n") if self.text.strip() else []
        return [line.strip() for line in lines if line.strip()]


class PrioritySelect(Select[int]):
    """Task priority dropdown with consistent options."""

    def __init__(
        self,
        value: TaskPriority = TaskPriority.MEDIUM,
        *,
        widget_id: str = "priority-select",
        **kwargs,
    ) -> None:
        options: Sequence[tuple[str, int]] = [
            (label, p.value) for p, label in PRIORITY_LABELS.items()
        ]

        initial_value = value.value if isinstance(value, TaskPriority) else value
        super().__init__(
            options=options,
            value=initial_value,
            id=widget_id,
            **kwargs,
        )


class TaskTypeSelect(Select[str]):
    """Task type dropdown (Pair/Auto)."""

    OPTIONS: Sequence[tuple[str, str]] = [
        ("Pair (interactive)", TaskType.PAIR.value),
        ("Auto (ACP)", TaskType.AUTO.value),
    ]

    def __init__(
        self,
        value: TaskType = TaskType.PAIR,
        *,
        disabled: bool = False,
        widget_id: str = "type-select",
        **kwargs,
    ) -> None:
        initial_value = value.value if isinstance(value, TaskType) else value
        super().__init__(
            options=self.OPTIONS,
            value=initial_value,
            disabled=disabled,
            id=widget_id,
            **kwargs,
        )


class StatusSelect(Select[str]):
    """Task status dropdown."""

    OPTIONS: Sequence[tuple[str, str]] = [
        ("Backlog", TaskStatus.BACKLOG.value),
        ("In Progress", TaskStatus.IN_PROGRESS.value),
        ("Review", TaskStatus.REVIEW.value),
        ("Done", TaskStatus.DONE.value),
    ]

    def __init__(
        self,
        value: TaskStatus = TaskStatus.BACKLOG,
        *,
        widget_id: str = "status-select",
        **kwargs,
    ) -> None:
        initial_value = value.value if isinstance(value, TaskStatus) else value
        super().__init__(
            options=self.OPTIONS,
            value=initial_value,
            id=widget_id,
            **kwargs,
        )


class AgentBackendSelect(Select[str]):
    """Agent backend dropdown with dynamic options."""

    def __init__(
        self,
        options: Sequence[tuple[str, str]] | None = None,
        value: str = "",
        *,
        widget_id: str = "agent-backend-select",
        allow_blank: bool = False,
        **kwargs,
    ) -> None:
        opts = options if options is not None else []
        super().__init__(
            options=opts,
            value=value,
            allow_blank=allow_blank,
            id=widget_id,
            **kwargs,
        )


class PairTerminalBackendSelect(Select[str]):
    """PAIR terminal backend dropdown."""

    OPTIONS: Sequence[tuple[str, str]] = [
        ("tmux", "tmux"),
        ("VS Code", "vscode"),
        ("Cursor", "cursor"),
    ]

    def __init__(
        self,
        value: str = "tmux",
        *,
        disabled: bool = False,
        widget_id: str = "pair-terminal-backend-select",
        **kwargs,
    ) -> None:
        valid_values = {backend.value for backend in PairTerminalBackend}
        initial_value = value if value in valid_values else "tmux"
        super().__init__(
            options=self.OPTIONS,
            value=initial_value,
            disabled=disabled,
            allow_blank=False,
            id=widget_id,
            **kwargs,
        )


class BaseBranchInput(Input):
    DEFAULT_PLACEHOLDER = "e.g. main, develop (blank = use default)"
    DEFAULT_MAX_LENGTH = 100

    def __init__(
        self,
        value: str = "",
        *,
        placeholder: str | None = None,
        max_length: int | None = None,
        widget_id: str = "base-branch-input",
        **kwargs,
    ) -> None:
        super().__init__(
            value=value or "",
            placeholder=placeholder or self.DEFAULT_PLACEHOLDER,
            max_length=max_length or self.DEFAULT_MAX_LENGTH,
            id=widget_id,
            **kwargs,
        )
