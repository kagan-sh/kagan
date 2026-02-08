"""Session record repository behavior."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from sqlmodel import select

from kagan.adapters.db.schema import Session
from kagan.core.models.enums import SessionStatus, SessionType
from kagan.core.time import utc_now

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession

    from kagan.adapters.db.repositories.base import ClosingAwareSessionFactory


class SessionRecordRepository:
    """Session record CRUD repository."""

    def __init__(self, session_factory: ClosingAwareSessionFactory) -> None:
        self._session_factory = session_factory
        self._lock = asyncio.Lock()

    def _get_session(self) -> AsyncSession:
        return self._session_factory()

    async def create_session_record(
        self,
        *,
        workspace_id: str,
        session_type: SessionType,
        external_id: str | None = None,
    ) -> Session:
        """Create a session record."""
        async with self._lock:
            async with self._get_session() as session:
                record = Session(
                    workspace_id=workspace_id,
                    session_type=session_type,
                    status=SessionStatus.ACTIVE,
                    external_id=external_id,
                    started_at=utc_now(),
                    ended_at=None,
                )
                session.add(record)
                await session.commit()
                await session.refresh(record)
                return record

    async def close_session_record(
        self,
        session_id: str,
        *,
        status: SessionStatus = SessionStatus.CLOSED,
    ) -> Session | None:
        """Close a session record."""
        async with self._lock:
            async with self._get_session() as session:
                record = await session.get(Session, session_id)
                if record is None:
                    return None
                record.status = status
                record.ended_at = utc_now()
                session.add(record)
                await session.commit()
                await session.refresh(record)
                return record

    async def close_session_by_external_id(
        self,
        external_id: str,
        *,
        status: SessionStatus = SessionStatus.CLOSED,
    ) -> Session | None:
        """Close a session record by external ID."""
        async with self._lock:
            async with self._get_session() as session:
                result = await session.execute(
                    select(Session).where(Session.external_id == external_id)
                )
                record = result.scalars().first()
                if record is None:
                    return None
                record.status = status
                record.ended_at = utc_now()
                session.add(record)
                await session.commit()
                await session.refresh(record)
                return record
