"""Async SQLAlchemy engine setup for SQLModel."""

from __future__ import annotations

from pathlib import Path

from sqlalchemy.ext.asyncio import AsyncEngine, create_async_engine
from sqlalchemy.pool import StaticPool
from sqlmodel import SQLModel

from kagan.paths import ensure_directories, get_database_path

LEGACY_TABLES = ("task_events", "agent_logs", "scratchpads")


async def create_db_engine(db_path: str | Path | None = None) -> AsyncEngine:
    """Create async SQLite engine with WAL mode."""
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

    async with engine.begin() as conn:
        await conn.exec_driver_sql("PRAGMA journal_mode=WAL")

    return engine


async def create_db_tables(engine: AsyncEngine) -> None:
    """Create all tables from SQLModel metadata."""
    async with engine.begin() as conn:
        for table in LEGACY_TABLES:
            await conn.exec_driver_sql(f"DROP TABLE IF EXISTS {table}")
        await conn.run_sync(SQLModel.metadata.create_all)


async def drop_db_tables(engine: AsyncEngine) -> None:
    """Drop all tables (for testing only)."""
    async with engine.begin() as conn:
        await conn.run_sync(SQLModel.metadata.drop_all)
