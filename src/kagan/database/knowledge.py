"""Knowledge base for storing learnings from completed tickets."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import aiosqlite


@dataclass
class KnowledgeEntry:
    """A knowledge base entry."""

    ticket_id: str
    summary: str
    tags: list[str]


class KnowledgeBase:
    """Simple full-text search knowledge base using SQLite FTS5."""

    def __init__(self, connection: aiosqlite.Connection) -> None:
        self._conn = connection

    async def add(self, ticket_id: str, summary: str, tags: list[str] | None = None) -> None:
        """Add a knowledge entry."""
        tags_str = " ".join(tags) if tags else ""
        await self._conn.execute(
            "INSERT OR REPLACE INTO knowledge (ticket_id, summary, tags) VALUES (?, ?, ?)",
            (ticket_id, summary, tags_str),
        )
        await self._conn.commit()

    async def search(self, query: str, limit: int = 5) -> list[KnowledgeEntry]:
        """Search knowledge base using full-text search."""
        async with self._conn.execute(
            """SELECT k.ticket_id, k.summary, k.tags 
               FROM knowledge k
               JOIN knowledge_fts fts ON k.rowid = fts.rowid
               WHERE knowledge_fts MATCH ?
               LIMIT ?""",
            (query, limit),
        ) as cursor:
            rows = await cursor.fetchall()
            return [
                KnowledgeEntry(
                    ticket_id=row[0],
                    summary=row[1],
                    tags=row[2].split() if row[2] else [],
                )
                for row in rows
            ]

    async def get_by_ticket(self, ticket_id: str) -> KnowledgeEntry | None:
        """Get knowledge entry for a specific ticket."""
        async with self._conn.execute(
            "SELECT ticket_id, summary, tags FROM knowledge WHERE ticket_id = ?",
            (ticket_id,),
        ) as cursor:
            row = await cursor.fetchone()
            if row:
                return KnowledgeEntry(
                    ticket_id=row[0],
                    summary=row[1],
                    tags=row[2].split() if row[2] else [],
                )
            return None
