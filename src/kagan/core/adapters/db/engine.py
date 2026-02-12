"""Async SQLAlchemy engine setup for SQLModel."""

from __future__ import annotations

import platform
import sys
from pathlib import Path

from sqlalchemy import event
from sqlalchemy.ext.asyncio import AsyncEngine, create_async_engine
from sqlalchemy.pool import StaticPool
from sqlmodel import SQLModel

from kagan.core.paths import ensure_directories, get_database_path


def _check_greenlet() -> None:
    """Verify greenlet is functional (required by SQLAlchemy async)."""
    try:
        import greenlet  # noqa: F401
    except (ImportError, OSError) as exc:
        py = f"Python {sys.version_info.major}.{sys.version_info.minor}"
        os_name = platform.system()
        raise RuntimeError(
            f"greenlet failed to load on {os_name} ({py}). "
            f"SQLAlchemy async requires a working greenlet installation.\n"
            f"Try: pip install --force-reinstall greenlet\n"
            f"Original error: {exc}"
        ) from exc


async def create_db_engine(db_path: str | Path | None = None) -> AsyncEngine:
    """Create async SQLite engine with WAL mode."""
    _check_greenlet()
    ensure_directories()
    resolved = Path(db_path) if db_path else get_database_path()
    db_path_str = str(resolved)
    if db_path_str == ":memory:":
        engine = create_async_engine(
            "sqlite+aiosqlite:///:memory:",
            echo=False,
            connect_args={"check_same_thread": False},
            poolclass=StaticPool,
        )
    else:
        db_path_path = Path(db_path_str)
        db_path_path.parent.mkdir(parents=True, exist_ok=True)
        engine = create_async_engine(
            f"sqlite+aiosqlite:///{db_path_path}",
            echo=False,
            connect_args={"check_same_thread": False},
        )

    @event.listens_for(engine.sync_engine, "connect")
    def _set_sqlite_pragmas(dbapi_connection, _connection_record) -> None:
        """Enable FK enforcement for every SQLite connection."""
        cursor = dbapi_connection.cursor()
        cursor.execute("PRAGMA foreign_keys=ON")
        cursor.close()

    async with engine.begin() as conn:
        await conn.exec_driver_sql("PRAGMA journal_mode=WAL")

    return engine


async def create_db_tables(engine: AsyncEngine) -> None:
    """Create all tables from SQLModel metadata."""
    async with engine.begin() as conn:
        await conn.run_sync(SQLModel.metadata.create_all)


async def drop_db_tables(engine: AsyncEngine) -> None:
    """Drop all tables (for testing only)."""
    async with engine.begin() as conn:
        await conn.run_sync(SQLModel.metadata.drop_all)
