"""Async database manager for Kagan state."""

import asyncio
from datetime import datetime
from pathlib import Path

import aiosqlite

from kagan.database.models import (
    Ticket,
    TicketCreate,
    TicketPriority,
    TicketStatus,
    TicketUpdate,
)

SCHEMA_PATH = Path(__file__).parent / "schema.sql"


class StateManager:
    """Async state manager for SQLite database operations."""

    def __init__(self, db_path: str | Path = ".kagan/state.db"):
        self.db_path = Path(db_path)
        self._connection: aiosqlite.Connection | None = None
        self._lock = asyncio.Lock()

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
            status=ticket.status,
            parent_id=ticket.parent_id,
        )

        async with self._lock:
            await conn.execute(
                """
                INSERT INTO tickets
                    (id, title, description, status, priority, parent_id, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
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
                    new_ticket.parent_id,
                    new_ticket.created_at.isoformat(),
                    new_ticket.updated_at.isoformat(),
                ),
            )
            await conn.commit()

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
            "status": update.status.value
            if isinstance(update.status, TicketStatus)
            else update.status,
            "parent_id": update.parent_id,
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

        return await self.get_ticket(ticket_id)

    async def delete_ticket(self, ticket_id: str) -> bool:
        """Delete a ticket. Returns True if deleted."""
        conn = await self._get_connection()
        async with self._lock:
            cursor = await conn.execute("DELETE FROM tickets WHERE id = ?", (ticket_id,))
            await conn.commit()
            return cursor.rowcount > 0

    async def move_ticket(self, ticket_id: str, new_status: TicketStatus) -> Ticket | None:
        """Move a ticket to a new status."""
        return await self.update_ticket(ticket_id, TicketUpdate(status=new_status))

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
        return Ticket(
            id=row["id"],
            title=row["title"],
            description=row["description"] or "",
            status=TicketStatus(row["status"]),
            priority=TicketPriority(row["priority"]),
            parent_id=row["parent_id"],
            created_at=datetime.fromisoformat(row["created_at"])
            if row["created_at"]
            else datetime.now(),
            updated_at=datetime.fromisoformat(row["updated_at"])
            if row["updated_at"]
            else datetime.now(),
        )
