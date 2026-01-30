"""Form building and validation for ticket details modal."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

from textual.widgets import Input, Select, TextArea

from kagan.database.models import (
    TicketCreate,
    TicketPriority,
    TicketStatus,
    TicketType,
    TicketUpdate,
)

if TYPE_CHECKING:
    from kagan.database.models import Ticket


def build_agent_options(app) -> list[tuple[str, str]]:
    """Build agent backend options from config."""
    options = [("Default", "")]
    kagan_app = getattr(app, "kagan_app", None) or app
    if hasattr(kagan_app, "config"):
        for name, agent in kagan_app.config.agents.items():
            if agent.active:
                options.append((agent.name, name))
    return options


def get_modal_title(ticket: Ticket | None, editing: bool, is_create: bool) -> str:
    if is_create:
        return "New Ticket"
    elif editing:
        return "Edit Ticket"
    else:
        return "Ticket Details"


def get_priority_label(ticket: Ticket | None) -> str:
    if not ticket:
        return "MED"
    priority = ticket.priority
    if isinstance(priority, int):
        priority = TicketPriority(priority)
    return priority.label


def get_priority_class(ticket: Ticket | None) -> str:
    if not ticket:
        return "badge-priority-medium"
    priority = ticket.priority
    if isinstance(priority, int):
        priority = TicketPriority(priority)
    return f"badge-priority-{priority.css_class}"


def get_type_label(ticket: Ticket | None) -> str:
    if not ticket:
        return "PAIR"
    ticket_type = ticket.ticket_type
    if isinstance(ticket_type, str):
        ticket_type = TicketType(ticket_type)
    if ticket_type == TicketType.AUTO:
        return "AUTO"
    return "PAIR"


def format_status(status: TicketStatus | str) -> str:
    if isinstance(status, str):
        status = TicketStatus(status)
    return status.value.replace("_", " ")


def has_review_data(ticket: Ticket | None) -> bool:
    if not ticket:
        return False
    return ticket.review_summary is not None or ticket.checks_passed is not None


def format_checks_badge(ticket: Ticket | None) -> str:
    if not ticket or ticket.checks_passed is None:
        return "Not Reviewed"
    return "Approved" if ticket.checks_passed else "Rejected"


def get_checks_class(ticket: Ticket | None) -> str:
    if not ticket or ticket.checks_passed is None:
        return "badge-checks-pending"
    return "badge-checks-passed" if ticket.checks_passed else "badge-checks-failed"


def parse_acceptance_criteria(modal) -> list[str]:
    """Parse acceptance criteria from TextArea (one per line)."""
    ac_input = modal.query_one("#ac-input", TextArea)
    lines = ac_input.text.strip().split("\n") if ac_input.text.strip() else []
    return [line.strip() for line in lines if line.strip()]


def validate_and_build_result(
    modal,
    ticket: Ticket | None,
    is_create: bool,
) -> TicketCreate | TicketUpdate | None:
    """Validate form and build result model. Returns None if validation fails."""
    title_input = modal.query_one("#title-input", Input)
    description_input = modal.query_one("#description-input", TextArea)
    priority_select: Select[int] = modal.query_one("#priority-select", Select)

    title = title_input.value.strip()
    if not title:
        modal.notify("Title is required", severity="error")
        title_input.focus()
        return None

    description = description_input.text

    priority_value = priority_select.value
    if priority_value is Select.BLANK:
        modal.notify("Priority is required", severity="error")
        priority_select.focus()
        return None
    priority = TicketPriority(cast("int", priority_value))

    type_select: Select[str] = modal.query_one("#type-select", Select)
    type_value = type_select.value
    if type_value is Select.BLANK:
        ticket_type = TicketType.PAIR
    else:
        ticket_type = TicketType(cast("str", type_value))

    agent_backend_select: Select[str] = modal.query_one("#agent-backend-select", Select)
    agent_backend_value = agent_backend_select.value
    agent_backend = str(agent_backend_value) if agent_backend_value is not Select.BLANK else ""

    acceptance_criteria = parse_acceptance_criteria(modal)

    if is_create:
        status_select: Select[str] = modal.query_one("#status-select", Select)
        status_value = status_select.value
        if status_value is Select.BLANK:
            modal.notify("Status is required", severity="error")
            status_select.focus()
            return None
        status = TicketStatus(cast("str", status_value))
        return TicketCreate(
            title=title,
            description=description,
            priority=priority,
            ticket_type=ticket_type,
            status=status,
            agent_backend=agent_backend or None,
            acceptance_criteria=acceptance_criteria,
        )
    else:
        return TicketUpdate(
            title=title,
            description=description,
            priority=priority,
            ticket_type=ticket_type,
            agent_backend=agent_backend or None,
            acceptance_criteria=acceptance_criteria,
        )


def reset_form_fields(modal, ticket: Ticket) -> None:
    """Reset form fields to original ticket values."""
    from textual.css.query import NoMatches

    try:
        modal.query_one("#title-input", Input).value = ticket.title
        modal.query_one("#description-input", TextArea).text = ticket.description or ""
        priority = ticket.priority
        if isinstance(priority, int):
            priority = TicketPriority(priority)
        modal.query_one("#priority-select", Select).value = priority.value
        ticket_type = ticket.ticket_type
        if isinstance(ticket_type, str):
            ticket_type = TicketType(ticket_type)
        modal.query_one("#type-select", Select).value = ticket_type.value
        modal.query_one("#agent-backend-select", Select).value = ticket.agent_backend or ""
        ac_text = "\n".join(ticket.acceptance_criteria) if ticket.acceptance_criteria else ""
        modal.query_one("#ac-input", TextArea).text = ac_text
    except NoMatches:
        pass
