"""Scratchpad repository behavior."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from sqlmodel import select

from kagan.adapters.db.schema import Scratch
from kagan.core.models.enums import ScratchType
from kagan.core.time import utc_now
from kagan.limits import SCRATCHPAD_LIMIT

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession

    from kagan.adapters.db.repositories.base import ClosingAwareSessionFactory


class ScratchRepository:
    """Scratchpad repository."""

    def __init__(self, session_factory: ClosingAwareSessionFactory) -> None:
        self._session_factory = session_factory
        self._lock = asyncio.Lock()

    def _get_session(self) -> AsyncSession:
        return self._session_factory()

    async def get_scratchpad(self, task_id: str) -> str:
        """Get scratchpad content for a task."""
        async with self._get_session() as session:
            result = await session.execute(
                select(Scratch).where(
                    Scratch.id == task_id,
                    Scratch.scratch_type == ScratchType.WORKSPACE_NOTES,
                )
            )
            scratchpad = result.scalars().first()
            if not scratchpad:
                return ""
            payload = scratchpad.payload or {}
            return str(payload.get("content", ""))

    async def update_scratchpad(self, task_id: str, content: str) -> None:
        """Update or create scratchpad content."""
        content = content[-SCRATCHPAD_LIMIT:] if len(content) > SCRATCHPAD_LIMIT else content

        async with self._lock:
            async with self._get_session() as session:
                result = await session.execute(
                    select(Scratch).where(
                        Scratch.id == task_id,
                        Scratch.scratch_type == ScratchType.WORKSPACE_NOTES,
                    )
                )
                scratchpad = result.scalars().first()
                if scratchpad:
                    scratchpad.payload = {"content": content}
                    scratchpad.updated_at = utc_now()
                else:
                    scratchpad = Scratch(
                        id=task_id,
                        scratch_type=ScratchType.WORKSPACE_NOTES,
                        payload={"content": content},
                    )
                    scratchpad.created_at = utc_now()
                    scratchpad.updated_at = utc_now()
                session.add(scratchpad)
                await session.commit()

    async def delete_scratchpad(self, task_id: str) -> None:
        """Delete scratchpad for a task."""
        async with self._lock:
            async with self._get_session() as session:
                result = await session.execute(
                    select(Scratch).where(
                        Scratch.id == task_id,
                        Scratch.scratch_type == ScratchType.WORKSPACE_NOTES,
                    )
                )
                scratchpad = result.scalars().first()
                if scratchpad:
                    await session.delete(scratchpad)
                    await session.commit()
