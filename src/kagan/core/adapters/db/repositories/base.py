"""Base types for DB repositories."""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker


class RepositoryClosing(Exception):
    """Raised when a DB operation is attempted during repository shutdown."""


class ClosingAwareSessionFactory:
    """Wrapper around ``async_sessionmaker`` with a shared closing flag."""

    def __init__(self, inner: async_sessionmaker[AsyncSession]) -> None:
        self._inner = inner
        self._closing = False

    def mark_closing(self) -> None:
        self._closing = True

    @property
    def closing(self) -> bool:
        return self._closing

    def __call__(self) -> AsyncSession:
        if self._closing:
            raise RepositoryClosing("Repository is shutting down")
        return self._inner()
