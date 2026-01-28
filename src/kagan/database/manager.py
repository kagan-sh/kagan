"""Async database manager for Kagan state."""

from __future__ import annotations

import asyncio
import json
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING

import aiosqlite

from kagan.database.models import (
    Ticket,
    TicketCreate,
    TicketPriority,
    TicketStatus,
    TicketType,
    TicketUpdate,
)

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
        """Notify listeners of ticket change via callback."""
        if self._on_change:
            self._on_change(ticket_id)

    async def initialize(self) -> None:
        """Initialize the database and create tables."""
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        async with self._lock:
            self._connection = await aiosqlite.connect(self.db_path)
            self._connection.row_factory = aiosqlite.Row

            # Enable WAL mode for better concurrency
            await self._connection.execute("PRAGMA journal_mode=WAL")

            # Execute schema using executescript for proper multi-statement handling
            schema = SCHEMA_PATH.read_text()
            await self._connection.executescript(schema)
            await self._connection.commit()

    async def close(self) -> None:
        """Close the database connection."""
        if self._connection:
            await self._connection.close()
            self._connection = None

    @property
    def connection(self) -> aiosqlite.Connection:
        """Get database connection (must be initialized first)."""
        assert self._connection is not None, "StateManager not initialized"
        return self._connection

    async def _get_connection(self) -> aiosqlite.Connection:
        """Get or create database connection."""
        if self._connection is None:
            await self.initialize()
        return self._connection  # type: ignore

    # Ticket CRUD operations

    async def create_ticket(self, ticket: TicketCreate) -> Ticket:
        """Create a new ticket."""
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
            check_command=ticket.check_command,
            review_summary=ticket.review_summary,
            checks_passed=ticket.checks_passed,
            session_active=ticket.session_active,
        )

        async with self._lock:
            await conn.execute(
                """
                INSERT INTO tickets
                    (id, title, description, status, priority, ticket_type,
                     assigned_hat, agent_backend, parent_id,
                     acceptance_criteria, check_command, review_summary,
                     checks_passed, session_active,
                     created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    new_ticket.id,
                    new_ticket.title,
                    new_ticket.description,
                    new_ticket.status.value
                    if isinstance(new_ticket.status, TicketStatus)
                    else new_ticket.status,
                    new_ticket.priority.value
                    if isinstance(new_ticket.priority, TicketPriority)
                    else new_ticket.priority,
                    new_ticket.ticket_type.value
                    if isinstance(new_ticket.ticket_type, TicketType)
                    else new_ticket.ticket_type,
                    new_ticket.assigned_hat,
                    new_ticket.agent_backend,
                    new_ticket.parent_id,
                    self._serialize_acceptance_criteria(new_ticket.acceptance_criteria),
                    new_ticket.check_command,
                    new_ticket.review_summary,
                    None
                    if new_ticket.checks_passed is None
                    else (1 if new_ticket.checks_passed else 0),
                    1 if new_ticket.session_active else 0,
                    new_ticket.created_at.isoformat(),
                    new_ticket.updated_at.isoformat(),
                ),
            )
            await conn.commit()

        self._notify_change(new_ticket.id)
        return new_ticket

    async def get_ticket(self, ticket_id: str) -> Ticket | None:
        """Get a ticket by ID."""
        conn = await self._get_connection()
        async with conn.execute("SELECT * FROM tickets WHERE id = ?", (ticket_id,)) as cursor:
            row = await cursor.fetchone()
            if row:
                return self._row_to_ticket(row)
        return None

    async def get_all_tickets(self) -> list[Ticket]:
        """Get all tickets ordered by status and priority."""
        conn = await self._get_connection()
        async with conn.execute(
            """
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
        ) as cursor:
            rows = await cursor.fetchall()
            return [self._row_to_ticket(row) for row in rows]

    async def get_tickets_by_status(self, status: TicketStatus) -> list[Ticket]:
        """Get all tickets with a specific status."""
        conn = await self._get_connection()
        status_value = status.value if isinstance(status, TicketStatus) else status
        async with conn.execute(
            """
            SELECT * FROM tickets
            WHERE status = ?
            ORDER BY priority DESC, created_at ASC
            """,
            (status_value,),
        ) as cursor:
            rows = await cursor.fetchall()
            return [self._row_to_ticket(row) for row in rows]

    async def update_ticket(self, ticket_id: str, update: TicketUpdate) -> Ticket | None:
        conn = await self._get_connection()

        fields = {
            "title": update.title,
            "description": update.description,
            "priority": update.priority.value
            if isinstance(update.priority, TicketPriority)
            else update.priority,
            "ticket_type": update.ticket_type.value
            if isinstance(update.ticket_type, TicketType)
            else update.ticket_type,
            "status": update.status.value
            if isinstance(update.status, TicketStatus)
            else update.status,
            "assigned_hat": update.assigned_hat,
            "agent_backend": update.agent_backend,
            "parent_id": update.parent_id,
            "acceptance_criteria": (
                self._serialize_acceptance_criteria(update.acceptance_criteria)
                if update.acceptance_criteria is not None
                else None
            ),
            "check_command": update.check_command,
            "review_summary": update.review_summary,
            "checks_passed": (
                (1 if update.checks_passed else 0) if update.checks_passed is not None else None
            ),
            "session_active": (
                (1 if update.session_active else 0) if update.session_active is not None else None
            ),
        }

        updates, values = [], []
        for field, value in fields.items():
            if value is not None:
                updates.append(f"{field} = ?")
                values.append(value)

        if not updates:
            return await self.get_ticket(ticket_id)

        values.append(ticket_id)
        async with self._lock:
            await conn.execute(f"UPDATE tickets SET {', '.join(updates)} WHERE id = ?", values)
            await conn.commit()

        self._notify_change(ticket_id)
        return await self.get_ticket(ticket_id)

    async def delete_ticket(self, ticket_id: str) -> bool:
        """Delete a ticket. Returns True if deleted."""
        conn = await self._get_connection()
        async with self._lock:
            cursor = await conn.execute("DELETE FROM tickets WHERE id = ?", (ticket_id,))
            await conn.commit()
            deleted = cursor.rowcount > 0
        if deleted:
            self._notify_change(ticket_id)
        return deleted

    async def move_ticket(self, ticket_id: str, new_status: TicketStatus) -> Ticket | None:
        """Move a ticket to a new status."""
        return await self.update_ticket(ticket_id, TicketUpdate(status=new_status))

    async def mark_session_active(self, ticket_id: str, active: bool) -> Ticket | None:
        """Mark a ticket's session as active or inactive."""
        return await self.update_ticket(ticket_id, TicketUpdate(session_active=active))

    async def set_review_summary(
        self, ticket_id: str, summary: str, checks_passed: bool | None
    ) -> Ticket | None:
        """Set the review summary and check status for a ticket."""
        return await self.update_ticket(
            ticket_id, TicketUpdate(review_summary=summary, checks_passed=checks_passed)
        )

    async def get_ticket_counts(self) -> dict[TicketStatus, int]:
        """Get count of tickets per status."""
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

    def _row_to_ticket(self, row: aiosqlite.Row) -> Ticket:
        """Convert a database row to a Ticket model."""
        # Handle ticket_type - default to PAIR for backward compatibility
        try:
            ticket_type_raw = row["ticket_type"]
            ticket_type = TicketType(ticket_type_raw) if ticket_type_raw else TicketType.PAIR
        except (KeyError, IndexError):
            ticket_type = TicketType.PAIR

        return Ticket(
            id=row["id"],
            title=row["title"],
            description=row["description"] or "",
            status=TicketStatus(row["status"]),
            priority=TicketPriority(row["priority"]),
            ticket_type=ticket_type,
            assigned_hat=row["assigned_hat"],
            agent_backend=row["agent_backend"],
            parent_id=row["parent_id"],
            acceptance_criteria=self._deserialize_acceptance_criteria(row["acceptance_criteria"]),
            check_command=row["check_command"],
            review_summary=row["review_summary"],
            checks_passed=None if row["checks_passed"] is None else bool(row["checks_passed"]),
            session_active=bool(row["session_active"]),
            created_at=datetime.fromisoformat(row["created_at"])
            if row["created_at"]
            else datetime.now(),
            updated_at=datetime.fromisoformat(row["updated_at"])
            if row["updated_at"]
            else datetime.now(),
        )

    @staticmethod
    def _serialize_acceptance_criteria(criteria: list[str]) -> str:
        """Serialize acceptance criteria list for storage."""
        return json.dumps(criteria)

    @staticmethod
    def _deserialize_acceptance_criteria(raw: str | None) -> list[str]:
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

    # Scratchpad operations

    async def get_scratchpad(self, ticket_id: str) -> str:
        """Get scratchpad content for a ticket."""
        conn = await self._get_connection()
        async with conn.execute(
            "SELECT content FROM scratchpads WHERE ticket_id = ?", (ticket_id,)
        ) as cursor:
            row = await cursor.fetchone()
            return row[0] if row else ""

    async def update_scratchpad(self, ticket_id: str, content: str) -> None:
        """Update or create scratchpad for a ticket."""
        conn = await self._get_connection()
        # Limit content size to prevent unbounded growth
        content = content[-50000:] if len(content) > 50000 else content
        async with self._lock:
            await conn.execute(
                """INSERT INTO scratchpads (ticket_id, content, updated_at)
                   VALUES (?, ?, CURRENT_TIMESTAMP)
                   ON CONFLICT(ticket_id) DO UPDATE SET
                   content = excluded.content, updated_at = CURRENT_TIMESTAMP""",
                (ticket_id, content),
            )
            await conn.commit()
        self._notify_change(ticket_id)

    async def delete_scratchpad(self, ticket_id: str) -> None:
        """Delete scratchpad when ticket is completed."""
        conn = await self._get_connection()
        async with self._lock:
            await conn.execute("DELETE FROM scratchpads WHERE ticket_id = ?", (ticket_id,))
            await conn.commit()

    # Knowledge operations

    async def add_knowledge(
        self, ticket_id: str, summary: str, tags: list[str] | None = None
    ) -> None:
        """Add knowledge entry from completed ticket."""
        conn = await self._get_connection()
        async with self._lock:
            await conn.execute(
                "INSERT OR REPLACE INTO knowledge (ticket_id, summary, tags) VALUES (?, ?, ?)",
                (ticket_id, summary, ",".join(tags or [])),
            )
            await conn.commit()

    async def search_knowledge(self, query: str, limit: int = 5) -> list[tuple[str, str]]:
        """Search knowledge base using FTS5."""
        conn = await self._get_connection()
        async with conn.execute(
            """SELECT ticket_id, summary FROM knowledge_fts
               WHERE knowledge_fts MATCH ? ORDER BY rank LIMIT ?""",
            (query, limit),
        ) as cursor:
            rows = await cursor.fetchall()
            return [(str(row[0]), str(row[1])) for row in rows]
