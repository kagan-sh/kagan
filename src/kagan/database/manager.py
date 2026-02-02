"""Async database manager for Kagan state."""

from __future__ import annotations

import asyncio
from pathlib import Path
from typing import TYPE_CHECKING, Any

import aiosqlite

from kagan.database import queries
from kagan.database.migrations import auto_migrate
from kagan.database.models import Ticket, TicketStatus
from kagan.limits import SCRATCHPAD_LIMIT

if TYPE_CHECKING:
    from collections.abc import Callable

SCHEMA_PATH = Path(__file__).parent / "schema.sql"


class StateManager:
    """Async state manager for SQLite database operations."""

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

        params = queries.build_insert_params(ticket, queries.serialize_acceptance_criteria)
        async with self._lock:
            await conn.execute(queries.INSERT_TICKET_SQL, params)
            await conn.commit()

        self._notify_change(ticket.id)
        self._notify_status_change(ticket.id, None, ticket.status)
        return ticket

    async def get_ticket(self, ticket_id: str) -> Ticket | None:
        conn = await self._get_connection()
        async with conn.execute("SELECT * FROM tickets WHERE id = ?", (ticket_id,)) as cursor:
            row = await cursor.fetchone()
            if row:
                return queries.row_to_ticket(row)
        return None

    async def get_all_tickets(self) -> list[Ticket]:
        conn = await self._get_connection()
        async with conn.execute(queries.SELECT_ALL_TICKETS_SQL) as cursor:
            rows = await cursor.fetchall()
            return [queries.row_to_ticket(row) for row in rows]

    async def get_tickets_by_status(self, status: TicketStatus) -> list[Ticket]:
        conn = await self._get_connection()
        status_value = status.value if isinstance(status, TicketStatus) else status
        async with conn.execute(queries.SELECT_BY_STATUS_SQL, (status_value,)) as cursor:
            rows = await cursor.fetchall()
            return [queries.row_to_ticket(row) for row in rows]

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
        updates, values = queries.build_update_params_from_dict(
            kwargs, queries.serialize_acceptance_criteria
        )

        if not updates:
            return await self.get_ticket(ticket_id)

        values.append(ticket_id)
        async with self._lock:
            await conn.execute(f"UPDATE tickets SET {', '.join(updates)} WHERE id = ?", values)
            await conn.commit()

        self._notify_change(ticket_id)

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
            await conn.execute(queries.UPSERT_SCRATCHPAD_SQL, (ticket_id, content))
            await conn.commit()
        self._notify_change(ticket_id)

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
            return [queries.row_to_ticket(row) for row in rows]

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
        self._notify_change(ticket_id)
