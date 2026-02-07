"""Form field factory for ticket modals.

Separates form generation logic from modal behavior.
"""

from __future__ import annotations

import platform
from enum import Enum, auto
from typing import TYPE_CHECKING

from textual.containers import Horizontal, Vertical
from textual.widgets import Label, Select, Static

from kagan.database.models import (
    Ticket,
    TicketPriority,
    TicketStatus,
    TicketType,
)
from kagan.ui.widgets.base import (
    AcceptanceCriteriaArea,
    AgentBackendSelect,
    DescriptionArea,
    PrioritySelect,
    StatusSelect,
    TicketTypeSelect,
    TitleInput,
)

if TYPE_CHECKING:
    from collections.abc import Sequence

    from textual.app import ComposeResult
    from textual.widget import Widget


class FormMode(Enum):
    """Form display mode."""

    CREATE = auto()
    VIEW = auto()
    EDIT = auto()


class TicketFormBuilder:
    """Factory for generating ticket form fields based on mode.

    Separates form generation from modal behavior for cleaner code.
    """

    @staticmethod
    def build_field_selects(
        ticket: Ticket | None,
        mode: FormMode,
        agent_options: Sequence[tuple[str, str]] | None = None,
    ) -> ComposeResult:
        """Build the priority/type/agent select row for edit mode.

        Args:
            ticket: The ticket being edited/viewed, or None for create.
            mode: Current form mode.
            agent_options: Available agent backend options.

        Yields:
            Widgets for the field select row.
        """
        if mode == FormMode.VIEW:
            return

        current_priority = ticket.priority if ticket else TicketPriority.MEDIUM

        if ticket:
            current_type = ticket.ticket_type
        else:
            current_type = TicketType.AUTO if platform.system() == "Windows" else TicketType.PAIR

        current_backend = ticket.agent_backend if ticket else ""

        with Horizontal(classes="field-row edit-fields", id="edit-fields-row"):
            with Vertical(classes="form-field field-third"):
                yield Label("Priority:", classes="form-label")
                yield PrioritySelect(value=current_priority)

            with Vertical(classes="form-field field-third"):
                yield Label("Type:", classes="form-label")
                # Disable type selector when editing existing ticket
                is_editing = ticket is not None
                yield TicketTypeSelect(value=current_type, disabled=is_editing)

            with Vertical(classes="form-field field-third"):
                yield Label("Agent:", classes="form-label")
                opts = agent_options if agent_options else [("Default", "")]
                yield AgentBackendSelect(options=opts, value=current_backend or "")

    @staticmethod
    def build_status_field(
        ticket: Ticket | None,
        mode: FormMode,
    ) -> ComposeResult:
        """Build the status select field (only shown in create mode).

        Args:
            ticket: The ticket being edited/viewed, or None for create.
            mode: Current form mode.

        Yields:
            Status field widget if in create mode.
        """
        if mode != FormMode.CREATE:
            return

        with Vertical(classes="form-field edit-fields", id="status-field"):
            yield Label("Status:", classes="form-label")
            yield StatusSelect(value=TicketStatus.BACKLOG)

    @staticmethod
    def build_title_field(
        ticket: Ticket | None,
        mode: FormMode,
    ) -> ComposeResult:
        """Build title field (view or edit).

        Args:
            ticket: The ticket being edited/viewed, or None for create.
            mode: Current form mode.

        Yields:
            Title field widgets.
        """
        title = ticket.title if ticket else ""

        # View mode: show static display
        yield Label("Title", classes="section-title view-only", id="title-section-label")
        yield Static(title, classes="ticket-title view-only", id="title-display", markup=False)

        # Edit mode: show input
        with Vertical(classes="form-field edit-fields", id="title-field"):
            yield TitleInput(value=title)

    @staticmethod
    def build_description_field(
        ticket: Ticket | None,
        mode: FormMode,
        editing: bool = False,
    ) -> ComposeResult:
        """Build description field with header.

        Args:
            ticket: The ticket being edited/viewed, or None for create.
            mode: Current form mode.
            editing: Whether currently in editing mode.

        Yields:
            Description field widgets.
        """
        description = (ticket.description if ticket else "") or "(No description)"

        with Horizontal(classes="description-header"):
            yield Label("Description", classes="section-title")
            yield Static("", classes="header-spacer")
            expand_text = "[F5] Full Editor" if editing else "[f] Expand"
            yield Static(expand_text, classes="expand-hint", id="expand-btn")

        # View mode display
        yield Static(
            description,
            classes="ticket-description view-only",
            id="description-content",
            markup=False,
        )

        # Edit mode input
        edit_text = ticket.description if ticket else ""
        with Vertical(classes="form-field edit-fields", id="description-field"):
            yield DescriptionArea(text=edit_text)

    @staticmethod
    def build_acceptance_criteria_field(
        ticket: Ticket | None,
        mode: FormMode,
    ) -> ComposeResult:
        """Build acceptance criteria section.

        Args:
            ticket: The ticket being edited/viewed, or None for create.
            mode: Current form mode.

        Yields:
            Acceptance criteria widgets.
        """
        # View mode: show existing criteria
        if ticket and ticket.acceptance_criteria:
            with Vertical(classes="acceptance-criteria-section view-only", id="ac-section"):
                yield Label("Acceptance Criteria", classes="section-title")
                for criterion in ticket.acceptance_criteria:
                    yield Static(f"  - {criterion}", classes="ac-item")

        # Edit mode: show textarea
        criteria = ticket.acceptance_criteria if ticket else []
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

                values["priority"] = TicketPriority(cast("int", priority_select.value))
        except Exception:
            pass

        try:
            type_select: Select[str] = container.query_one("#type-select", Select)
            if type_select.value is not Select.BLANK:
                values["ticket_type"] = TicketType(str(type_select.value))
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
                values["status"] = TicketStatus(str(status_select.value))
        except Exception:
            pass

        try:
            ac_input = container.query_one("#ac-input", AcceptanceCriteriaArea)
            values["acceptance_criteria"] = ac_input.get_criteria()
        except Exception:
            pass

        return values

    @staticmethod
    def reset_form_to_ticket(container: Widget, ticket: Ticket) -> None:
        """Reset form fields to match ticket values.

        Args:
            container: The container widget holding form fields.
            ticket: The ticket to reset values from.
        """
        from textual.widgets import Input, Select, TextArea

        from kagan.ui.utils import safe_query_one

        if title_input := safe_query_one(container, "#title-input", Input):
            title_input.value = ticket.title

        if desc_input := safe_query_one(container, "#description-input", TextArea):
            desc_input.text = ticket.description or ""

        if priority_select := safe_query_one(container, "#priority-select", Select):
            priority_select.value = ticket.priority.value

        if type_select := safe_query_one(container, "#type-select", Select):
            type_select.value = ticket.ticket_type.value

        if agent_select := safe_query_one(container, "#agent-backend-select", Select):
            agent_select.value = ticket.agent_backend or ""

        if ac_input := safe_query_one(container, "#ac-input", TextArea):
            ac_text = "\n".join(ticket.acceptance_criteria) if ticket.acceptance_criteria else ""
            ac_input.text = ac_text
