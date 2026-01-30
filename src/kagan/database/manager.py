"""Async database manager for Kagan state."""

from __future__ import annotations

import asyncio
from pathlib import Path
from typing import TYPE_CHECKING

import aiosqlite

from kagan.database import queries
from kagan.database.models import Ticket, TicketCreate, TicketStatus, TicketUpdate
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

    def _notify_change(self, ticket_id: str) -> None:
        if self._on_change:
            self._on_change(ticket_id)

    async def initialize(self) -> None:
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        async with self._lock:
            self._connection = await aiosqlite.connect(self.db_path)
            self._connection.row_factory = aiosqlite.Row
            await self._connection.execute("PRAGMA journal_mode=WAL")
            schema = SCHEMA_PATH.read_text()
            await self._connection.executescript(schema)
            await self._connection.commit()

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

    async def create_ticket(self, ticket: TicketCreate) -> Ticket:
        conn = await self._get_connection()
        new_ticket = Ticket(
            title=ticket.title,
            description=ticket.description,
            priority=ticket.priority,
            ticket_type=ticket.ticket_type,
            status=ticket.status,
            assigned_hat=ticket.assigned_hat,
            agent_backend=ticket.agent_backend,
            parent_id=ticket.parent_id,
            acceptance_criteria=ticket.acceptance_criteria,
            review_summary=ticket.review_summary,
            checks_passed=ticket.checks_passed,
            session_active=ticket.session_active,
        )

        params = queries.build_insert_params(new_ticket, queries.serialize_acceptance_criteria)
        async with self._lock:
            await conn.execute(queries.INSERT_TICKET_SQL, params)
            await conn.commit()

        self._notify_change(new_ticket.id)
        return new_ticket

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

    async def update_ticket(self, ticket_id: str, update: TicketUpdate) -> Ticket | None:
        conn = await self._get_connection()
        updates, values = queries.build_update_params(update, queries.serialize_acceptance_criteria)

        if not updates:
            return await self.get_ticket(ticket_id)

        values.append(ticket_id)
        async with self._lock:
            await conn.execute(f"UPDATE tickets SET {', '.join(updates)} WHERE id = ?", values)
            await conn.commit()

        self._notify_change(ticket_id)
        return await self.get_ticket(ticket_id)

    async def delete_ticket(self, ticket_id: str) -> bool:
        conn = await self._get_connection()
        async with self._lock:
            cursor = await conn.execute("DELETE FROM tickets WHERE id = ?", (ticket_id,))
            await conn.commit()
            deleted = cursor.rowcount > 0
        if deleted:
            self._notify_change(ticket_id)
        return deleted

    async def move_ticket(self, ticket_id: str, new_status: TicketStatus) -> Ticket | None:
        return await self.update_ticket(ticket_id, TicketUpdate(status=new_status))

    async def mark_session_active(self, ticket_id: str, active: bool) -> Ticket | None:
        return await self.update_ticket(ticket_id, TicketUpdate(session_active=active))

    async def set_review_summary(
        self, ticket_id: str, summary: str, checks_passed: bool | None
    ) -> Ticket | None:
        return await self.update_ticket(
            ticket_id, TicketUpdate(review_summary=summary, checks_passed=checks_passed)
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
