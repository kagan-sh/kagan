"""SQL query helpers and row conversion for database operations."""

from __future__ import annotations

import json
from datetime import datetime
from typing import TYPE_CHECKING, Any, cast

from kagan.database.models import Ticket, TicketPriority, TicketStatus, TicketType

if TYPE_CHECKING:
    from collections.abc import Callable

    import aiosqlite


def row_to_ticket(row: aiosqlite.Row) -> Ticket:
    """Convert a database row to a Ticket model."""
    ticket_type_raw = row["ticket_type"]
    ticket_type = TicketType(ticket_type_raw) if ticket_type_raw else TicketType.PAIR

    return Ticket(
        id=row["id"],
        title=row["title"],
        description=row["description"] or "",
        status=TicketStatus(row["status"]),
        priority=TicketPriority(row["priority"]),
        ticket_type=ticket_type,
        assigned_hat=cast("str | None", row["assigned_hat"]),
        agent_backend=cast("str | None", row["agent_backend"]),
        parent_id=cast("str | None", row["parent_id"]),
        acceptance_criteria=deserialize_acceptance_criteria(
            cast("str | None", row["acceptance_criteria"])
        ),
        review_summary=cast("str | None", row["review_summary"]),
        checks_passed=None if row["checks_passed"] is None else bool(row["checks_passed"]),
        session_active=bool(row["session_active"]),
        total_iterations=row["total_iterations"] or 0,
        created_at=(
            datetime.fromisoformat(row["created_at"]) if row["created_at"] else datetime.now()
        ),
        updated_at=(
            datetime.fromisoformat(row["updated_at"]) if row["updated_at"] else datetime.now()
        ),
    )


def serialize_acceptance_criteria(criteria: list[str]) -> str:
    """Serialize acceptance criteria list for storage."""
    return json.dumps(criteria)


def deserialize_acceptance_criteria(raw: str | None) -> list[str]:
    """Deserialize acceptance criteria from storage."""
    if not raw:
        return []
    try:
        parsed = json.loads(raw)
        if isinstance(parsed, list):
            return [str(item) for item in parsed]
    except json.JSONDecodeError:
        return [raw]
    return []


def build_update_params_from_dict(
    updates: dict[str, Any],
    serialize_fn: Callable[[list[str]], str],
) -> tuple[list[str], list[object | None]]:
    """Build SQL UPDATE parameters from a dict of field->value.

    Args:
        updates: Dict of field names to new values.
        serialize_fn: Function to serialize acceptance_criteria to string.

    Returns:
        Tuple of (update clauses like "field = ?", values list).
    """
    clauses: list[str] = []
    values: list[object | None] = []

    for field, value in updates.items():
        # Convert enums and special types to DB format
        is_enum_field = field in ("status", "ticket_type", "priority")
        if is_enum_field and hasattr(value, "value"):
            db_value = value.value
        elif field == "acceptance_criteria" and value is not None:
            db_value = serialize_fn(value)
        elif field in ("checks_passed", "session_active") and value is not None:
            db_value = 1 if value else 0
        else:
            db_value = value

        clauses.append(f"{field} = ?")
        values.append(db_value)

    return clauses, values


def build_insert_params(
    ticket: Ticket,
    serialize_fn: Callable[[list[str]], str],
) -> tuple[object, ...]:
    """Build INSERT parameters for a new ticket."""
    return (
        ticket.id,
        ticket.title,
        ticket.description,
        ticket.status.value if isinstance(ticket.status, TicketStatus) else ticket.status,
        ticket.priority.value if isinstance(ticket.priority, TicketPriority) else ticket.priority,
        (
            ticket.ticket_type.value
            if isinstance(ticket.ticket_type, TicketType)
            else ticket.ticket_type
        ),
        ticket.assigned_hat,
        ticket.agent_backend,
        ticket.parent_id,
        serialize_fn(ticket.acceptance_criteria),
        ticket.review_summary,
        None if ticket.checks_passed is None else (1 if ticket.checks_passed else 0),
        1 if ticket.session_active else 0,
        ticket.total_iterations,
        ticket.created_at.isoformat(),
        ticket.updated_at.isoformat(),
    )


# SQL Statements
INSERT_TICKET_SQL = """
INSERT INTO tickets
    (id, title, description, status, priority, ticket_type,
     assigned_hat, agent_backend, parent_id,
     acceptance_criteria, review_summary,
     checks_passed, session_active, total_iterations,
     created_at, updated_at)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"""

SELECT_ALL_TICKETS_SQL = """
SELECT * FROM tickets
ORDER BY
    CASE status
        WHEN 'BACKLOG' THEN 0
        WHEN 'IN_PROGRESS' THEN 1
        WHEN 'REVIEW' THEN 2
        WHEN 'DONE' THEN 3
    END,
    priority DESC,
    created_at ASC
"""

SELECT_BY_STATUS_SQL = """
SELECT * FROM tickets
WHERE status = ?
ORDER BY priority DESC, created_at ASC
"""

UPSERT_SCRATCHPAD_SQL = """
INSERT INTO scratchpads (ticket_id, content, updated_at)
VALUES (?, ?, CURRENT_TIMESTAMP)
ON CONFLICT(ticket_id) DO UPDATE SET
content = excluded.content, updated_at = CURRENT_TIMESTAMP
"""
