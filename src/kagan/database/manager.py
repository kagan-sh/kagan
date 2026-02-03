"""Async database manager for Kagan state."""

from __future__ import annotations

import asyncio
import json
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, Any

import aiosqlite

from kagan.database.migrations import auto_migrate
from kagan.database.models import AgentLogEntry, Ticket, TicketEvent, TicketStatus
from kagan.limits import SCRATCHPAD_LIMIT

if TYPE_CHECKING:
    from collections.abc import Callable

SCHEMA_PATH = Path(__file__).parent / "schema.sql"


class StateManager:
    """Async state manager for SQLite database operations."""

    # SQL Query Constants
    INSERT_TICKET_SQL = """
    INSERT INTO tickets
        (id, title, description, status, priority, ticket_type,
         assigned_hat, agent_backend, parent_id,
         acceptance_criteria, review_summary,
         checks_passed, session_active, total_iterations,
         merge_failed, merge_error, merge_readiness,
         last_error, block_reason,
         created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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

    INSERT_AGENT_LOG_SQL = """
    INSERT INTO agent_logs (ticket_id, log_type, iteration, content)
    VALUES (?, ?, ?, ?)
    """

    SELECT_AGENT_LOGS_SQL = """
    SELECT id, ticket_id, log_type, iteration, content, created_at
    FROM agent_logs
    WHERE ticket_id = ? AND log_type = ?
    ORDER BY iteration ASC, created_at ASC
    """

    DELETE_AGENT_LOGS_SQL = """
    DELETE FROM agent_logs WHERE ticket_id = ?
    """

    INSERT_TICKET_EVENT_SQL = """
    INSERT INTO ticket_events (ticket_id, event_type, message)
    VALUES (?, ?, ?)
    """

    SELECT_TICKET_EVENTS_SQL = """
    SELECT id, ticket_id, event_type, message, created_at
    FROM ticket_events
    WHERE ticket_id = ?
    ORDER BY created_at DESC, id DESC
    LIMIT ?
    """

    def __init__(
        self,
        db_path: str | Path = ".kagan/state.db",
        on_change: Callable[[str], None] | None = None,
    ):
        self.db_path = Path(db_path)
        self._connection: aiosqlite.Connection | None = None
        self._lock = asyncio.Lock()
        self._on_change = on_change
        # Status change callback for reactive scheduler
        self._on_status_change: (
            Callable[[str, TicketStatus | None, TicketStatus | None], None] | None
        ) = None

    def set_status_change_callback(
        self, callback: Callable[[str, TicketStatus | None, TicketStatus | None], None] | None
    ) -> None:
        """Set callback for ticket status changes.

        Callback receives (ticket_id, old_status, new_status).
        new_status is None when ticket is deleted.
        old_status is None when ticket is created.
        """
        self._on_status_change = callback

    def _notify_change(self, ticket_id: str) -> None:
        if self._on_change:
            self._on_change(ticket_id)

    def _notify_status_change(
        self, ticket_id: str, old_status: TicketStatus | None, new_status: TicketStatus | None
    ) -> None:
        if self._on_status_change:
            self._on_status_change(ticket_id, old_status, new_status)

    async def initialize(self) -> None:
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        async with self._lock:
            self._connection = await aiosqlite.connect(self.db_path)
            self._connection.row_factory = aiosqlite.Row
            await self._connection.execute("PRAGMA journal_mode=WAL")

            # Auto-migrate database to match schema.sql
            # This runs on every boot (standard pattern for CLI tools like gh, claude, etc.)
            schema = SCHEMA_PATH.read_text()
            await auto_migrate(self._connection, schema, self.db_path)

    async def close(self) -> None:
        if self._connection:
            await self._connection.close()
            self._connection = None

    @property
    def connection(self) -> aiosqlite.Connection:
        assert self._connection is not None, "StateManager not initialized"
        return self._connection

    async def _get_connection(self) -> aiosqlite.Connection:
        if self._connection is None:
            await self.initialize()
        assert self._connection is not None, "Failed to initialize connection"
        return self._connection

    async def create_ticket(self, ticket: Ticket) -> Ticket:
        """Create a new ticket in the database.

        Args:
            ticket: A Ticket instance (typically created via Ticket.create()).

        Returns:
            The created Ticket.
        """
        conn = await self._get_connection()

        params = ticket.to_insert_params()
        async with self._lock:
            await conn.execute(self.INSERT_TICKET_SQL, params)
            await conn.commit()

        self._notify_change(ticket.id)
        self._notify_status_change(ticket.id, None, ticket.status)
        return ticket

    async def get_ticket(self, ticket_id: str) -> Ticket | None:
        conn = await self._get_connection()
        async with conn.execute("SELECT * FROM tickets WHERE id = ?", (ticket_id,)) as cursor:
            row = await cursor.fetchone()
            if row:
                return Ticket.from_row(row)
        return None

    async def get_all_tickets(self) -> list[Ticket]:
        conn = await self._get_connection()
        async with conn.execute(self.SELECT_ALL_TICKETS_SQL) as cursor:
            rows = await cursor.fetchall()
            return [Ticket.from_row(row) for row in rows]

    async def get_tickets_by_status(self, status: TicketStatus) -> list[Ticket]:
        conn = await self._get_connection()
        status_value = status.value if isinstance(status, TicketStatus) else status
        async with conn.execute(self.SELECT_BY_STATUS_SQL, (status_value,)) as cursor:
            rows = await cursor.fetchall()
            return [Ticket.from_row(row) for row in rows]

    async def update_ticket(self, ticket_id: str, **kwargs: Any) -> Ticket | None:
        """Update a ticket with the given fields.

        Args:
            ticket_id: The ticket ID to update.
            **kwargs: Fields to update (e.g., status=TicketStatus.DONE, title="New title").

        Returns:
            The updated Ticket, or None if not found.
        """
        if not kwargs:
            return await self.get_ticket(ticket_id)

        # Get old status if we're changing status
        old_status: TicketStatus | None = None
        new_status: TicketStatus | None = None
        if "status" in kwargs:
            old_ticket = await self.get_ticket(ticket_id)
            if old_ticket:
                old_status = old_ticket.status
            new_status = kwargs["status"]
            if isinstance(new_status, str):
                new_status = TicketStatus(new_status)

        conn = await self._get_connection()

        # Build UPDATE parameters from kwargs
        clauses: list[str] = []
        values: list[object | None] = []
        for field, value in kwargs.items():
            # Convert enums and special types to DB format
            is_enum_field = field in ("status", "ticket_type", "priority", "merge_readiness")
            if is_enum_field and hasattr(value, "value"):
                db_value = value.value
            elif field == "acceptance_criteria" and value is not None:
                db_value = json.dumps(value)
            elif field in ("checks_passed", "session_active", "merge_failed") and value is not None:
                db_value = 1 if value else 0
            else:
                db_value = value
            clauses.append(f"{field} = ?")
            values.append(db_value)

        if not clauses:
            return await self.get_ticket(ticket_id)

        values.append(ticket_id)
        async with self._lock:
            await conn.execute(f"UPDATE tickets SET {', '.join(clauses)} WHERE id = ?", values)
            await conn.commit()

        # Notify status change if status was updated
        if new_status is not None and old_status != new_status:
            self._notify_status_change(ticket_id, old_status, new_status)

        return await self.get_ticket(ticket_id)

    async def delete_ticket(self, ticket_id: str) -> bool:
        # Get old status before deletion
        old_ticket = await self.get_ticket(ticket_id)
        old_status = old_ticket.status if old_ticket else None

        conn = await self._get_connection()
        async with self._lock:
            cursor = await conn.execute("DELETE FROM tickets WHERE id = ?", (ticket_id,))
            await conn.commit()
            deleted = cursor.rowcount > 0

        if deleted:
            self._notify_change(ticket_id)
            self._notify_status_change(ticket_id, old_status, None)
        return deleted

    async def move_ticket(self, ticket_id: str, new_status: TicketStatus) -> Ticket | None:
        return await self.update_ticket(ticket_id, status=new_status)

    async def mark_session_active(self, ticket_id: str, active: bool) -> Ticket | None:
        return await self.update_ticket(ticket_id, session_active=active)

    async def set_review_summary(
        self, ticket_id: str, summary: str, checks_passed: bool | None
    ) -> Ticket | None:
        return await self.update_ticket(
            ticket_id, review_summary=summary, checks_passed=checks_passed
        )

    async def get_ticket_counts(self) -> dict[TicketStatus, int]:
        conn = await self._get_connection()
        counts = {status: 0 for status in TicketStatus}

        async with conn.execute(
            "SELECT status, COUNT(*) as count FROM tickets GROUP BY status"
        ) as cursor:
            rows = await cursor.fetchall()
            for row in rows:
                status = TicketStatus(row["status"])
                counts[status] = row["count"]

        return counts

    async def get_scratchpad(self, ticket_id: str) -> str:
        conn = await self._get_connection()
        async with conn.execute(
            "SELECT content FROM scratchpads WHERE ticket_id = ?", (ticket_id,)
        ) as cursor:
            row = await cursor.fetchone()
            return row[0] if row else ""

    async def update_scratchpad(self, ticket_id: str, content: str) -> None:
        conn = await self._get_connection()
        content = content[-SCRATCHPAD_LIMIT:] if len(content) > SCRATCHPAD_LIMIT else content
        async with self._lock:
            await conn.execute(self.UPSERT_SCRATCHPAD_SQL, (ticket_id, content))
            await conn.commit()

    async def delete_scratchpad(self, ticket_id: str) -> None:
        conn = await self._get_connection()
        async with self._lock:
            await conn.execute("DELETE FROM scratchpads WHERE ticket_id = ?", (ticket_id,))
            await conn.commit()

    async def search_tickets(self, query: str) -> list[Ticket]:
        """Full-text search on title, description, and ID."""
        if not query or not query.strip():
            return []

        conn = await self._get_connection()
        query = query.strip()
        like_pattern = f"%{query}%"

        sql = """
            SELECT * FROM tickets
            WHERE id = ? OR title LIKE ? OR description LIKE ?
            ORDER BY
                CASE
                    WHEN id = ? THEN 0
                    WHEN title LIKE ? THEN 1
                    ELSE 2
                END,
                updated_at DESC
        """
        params = (query, like_pattern, like_pattern, query, like_pattern)

        async with conn.execute(sql, params) as cursor:
            rows = await cursor.fetchall()
            return [Ticket.from_row(row) for row in rows]

    async def increment_total_iterations(self, ticket_id: str) -> None:
        """Increment the total_iterations counter for a ticket.

        This is a lifetime odometer that monotonically increases to track
        total cost/iterations for a ticket.

        Args:
            ticket_id: The ticket ID to update.
        """
        conn = await self._get_connection()
        async with self._lock:
            await conn.execute(
                "UPDATE tickets SET total_iterations = total_iterations + 1 WHERE id = ?",
                (ticket_id,),
            )
            await conn.commit()

    async def append_agent_log(
        self, ticket_id: str, log_type: str, iteration: int, content: str
    ) -> None:
        """Append a log entry for agent execution.

        Args:
            ticket_id: The ticket ID
            log_type: Either 'implementation' or 'review'
            iteration: The iteration number
            content: The full log content
        """
        conn = await self._get_connection()
        async with self._lock:
            await conn.execute(
                self.INSERT_AGENT_LOG_SQL,
                (ticket_id, log_type, iteration, content),
            )
            await conn.commit()

    async def get_agent_logs(self, ticket_id: str, log_type: str) -> list[AgentLogEntry]:
        """Get all log entries for a ticket and log type.

        Args:
            ticket_id: The ticket ID
            log_type: Either 'implementation' or 'review'

        Returns:
            List of AgentLogEntry objects ordered by iteration
        """
        conn = await self._get_connection()
        cursor = await conn.execute(
            self.SELECT_AGENT_LOGS_SQL,
            (ticket_id, log_type),
        )
        rows = await cursor.fetchall()
        return [
            AgentLogEntry(
                id=row[0],
                ticket_id=row[1],
                log_type=row[2],
                iteration=row[3],
                content=row[4],
                created_at=datetime.fromisoformat(row[5]) if row[5] else datetime.now(),
            )
            for row in rows
        ]

    async def clear_agent_logs(self, ticket_id: str) -> None:
        """Clear all agent logs for a ticket.

        Called when a ticket is retried to start fresh.

        Args:
            ticket_id: The ticket ID
        """
        conn = await self._get_connection()
        async with self._lock:
            await conn.execute(self.DELETE_AGENT_LOGS_SQL, (ticket_id,))
            await conn.commit()

    async def append_ticket_event(self, ticket_id: str, event_type: str, message: str) -> None:
        """Append an audit event for a ticket.

        Args:
            ticket_id: The ticket ID
            event_type: Short event category (e.g., merge, review, policy)
            message: Human-readable details
        """
        conn = await self._get_connection()
        async with self._lock:
            await conn.execute(self.INSERT_TICKET_EVENT_SQL, (ticket_id, event_type, message))
            await conn.commit()

    async def get_ticket_events(self, ticket_id: str, limit: int = 20) -> list[TicketEvent]:
        """Get recent audit events for a ticket.

        Args:
            ticket_id: The ticket ID
            limit: Max number of events to return

        Returns:
            List of TicketEvent entries (most recent first)
        """
        conn = await self._get_connection()
        cursor = await conn.execute(self.SELECT_TICKET_EVENTS_SQL, (ticket_id, limit))
        rows = await cursor.fetchall()
        return [
            TicketEvent(
                id=row[0],
                ticket_id=row[1],
                event_type=row[2],
                message=row[3],
                created_at=datetime.fromisoformat(row[4]) if row[4] else datetime.now(),
            )
            for row in rows
        ]
