"""Reusable base widget classes for forms and modals.

Provides consistent styling and configuration for common form components.
Based on patterns from JiraTUI.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.widgets import Input, Select, Static, TextArea

from kagan.constants import PRIORITY_LABELS
from kagan.database.models import TicketPriority, TicketStatus, TicketType

if TYPE_CHECKING:
    from collections.abc import Sequence


class TitleInput(Input):
    """Reusable title input with consistent styling and validation."""

    DEFAULT_PLACEHOLDER = "Enter ticket title..."
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
    """Ticket priority dropdown with consistent options."""

    def __init__(
        self,
        value: TicketPriority = TicketPriority.MEDIUM,
        *,
        widget_id: str = "priority-select",
        **kwargs,
    ) -> None:
        options: Sequence[tuple[str, int]] = [
            (label, p.value) for p, label in PRIORITY_LABELS.items()
        ]
        # Ensure value is int for Select
        initial_value = value.value if isinstance(value, TicketPriority) else value
        super().__init__(
            options=options,
            value=initial_value,
            id=widget_id,
            **kwargs,
        )


class TicketTypeSelect(Select[str]):
    """Ticket type dropdown (Pair/Auto)."""

    OPTIONS: Sequence[tuple[str, str]] = [
        ("Pair (tmux)", TicketType.PAIR.value),
        ("Auto (ACP)", TicketType.AUTO.value),
    ]

    def __init__(
        self,
        value: TicketType = TicketType.PAIR,
        *,
        disabled: bool = False,
        widget_id: str = "type-select",
        **kwargs,
    ) -> None:
        # Ensure value is str for Select
        initial_value = value.value if isinstance(value, TicketType) else value
        super().__init__(
            options=self.OPTIONS,
            value=initial_value,
            disabled=disabled,
            id=widget_id,
            **kwargs,
        )


class StatusSelect(Select[str]):
    """Ticket status dropdown."""

    OPTIONS: Sequence[tuple[str, str]] = [
        ("Backlog", TicketStatus.BACKLOG.value),
        ("In Progress", TicketStatus.IN_PROGRESS.value),
        ("Review", TicketStatus.REVIEW.value),
        ("Done", TicketStatus.DONE.value),
    ]

    def __init__(
        self,
        value: TicketStatus = TicketStatus.BACKLOG,
        *,
        widget_id: str = "status-select",
        **kwargs,
    ) -> None:
        # Ensure value is str for Select
        initial_value = value.value if isinstance(value, TicketStatus) else value
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
        allow_blank: bool = True,
        **kwargs,
    ) -> None:
        # Default options if none provided
        opts = options if options is not None else [("Default", "")]
        super().__init__(
            options=opts,
            value=value,
            allow_blank=allow_blank,
            id=widget_id,
            **kwargs,
        )


class ReadOnlyField(Static):
    """A read-only display field for view mode."""

    def __init__(
        self,
        content: str = "",
        *,
        label: str = "",
        widget_id: str | None = None,
        classes: str = "",
        **kwargs,
    ) -> None:
        super().__init__(
            content,
            id=widget_id,
            classes=f"readonly-field {classes}".strip(),
            **kwargs,
        )
        self._label = label

    @property
    def label(self) -> str:
        """Return the field label."""
        return self._label
