"""Shared helpers for creating DB sessions."""

from __future__ import annotations

from typing import TYPE_CHECKING, Protocol

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession


class AsyncSessionFactory(Protocol):
    """Callable session provider used by DB-backed services."""

    def __call__(self) -> AsyncSession: ...


def get_session(session_factory: AsyncSessionFactory) -> AsyncSession:
    """Create an ``AsyncSession`` from a closing-aware factory."""
    return session_factory()


def get_required_session(
    session_factory: AsyncSessionFactory | None,
    *,
    error_message: str,
) -> AsyncSession:
    """Create an ``AsyncSession`` and fail fast if the factory is unavailable."""
    if session_factory is None:
        raise RuntimeError(error_message)
    return session_factory()
