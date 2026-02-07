from __future__ import annotations

import sqlite3
from typing import TYPE_CHECKING

from kagan.adapters.db.repositories import TaskRepository

if TYPE_CHECKING:
    from pathlib import Path


def _create_legacy_db(db_path: Path) -> None:
    conn = sqlite3.connect(db_path)
    conn.execute("CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
    conn.execute(
        """
        CREATE TABLE tasks (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            parent_id TEXT,
            title TEXT NOT NULL,
            description TEXT DEFAULT '',
            status TEXT NOT NULL DEFAULT 'BACKLOG',
            priority INTEGER NOT NULL DEFAULT 1,
            task_type TEXT NOT NULL DEFAULT 'PAIR',
            assigned_hat TEXT,
            agent_backend TEXT,
            base_branch TEXT,
            acceptance_criteria JSON NOT NULL DEFAULT '[]',
            created_at DATETIME NOT NULL,
            updated_at DATETIME NOT NULL
        )
        """
    )
    conn.commit()
    conn.close()


async def test_initialize_adds_terminal_backend_column_for_legacy_db(tmp_path: Path) -> None:
    db_path = tmp_path / "legacy.db"
    _create_legacy_db(db_path)

    repo = TaskRepository(db_path)
    await repo.initialize()

    assert repo._engine is not None
    async with repo._engine.begin() as conn:
        result = await conn.exec_driver_sql("PRAGMA table_info(tasks)")
        columns = {str(row[1]) for row in result.fetchall()}

    await repo.close()
    assert "terminal_backend" in columns
