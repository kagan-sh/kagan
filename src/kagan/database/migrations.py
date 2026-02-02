"""Declarative schema migration for SQLite using full table recreate.

Strategy: Compare current DB against pristine in-memory DB created from
schema.sql. For any differences, recreate the table using SQLite's 12-step
procedure (create new → copy data → drop old → rename).

This approach:
- Requires no migration files (schema.sql is source of truth)
- Auto-detects new tables, column additions/removals, type changes
- Safely handles SQLite's ALTER TABLE limitations
- Uses user_version pragma for tracking (clear Alembic upgrade path)
- Runs automatically on boot (pattern used by CLI tools like Claude, gh, etc.)

Decision rationale: Research shows developer CLI tools (Cursor, Claude Code,
VS Code, Obsidian) universally auto-migrate silently. Explicit migration
commands are designed for production databases with teams—not single-user
SQLite files.
"""

from __future__ import annotations

import contextlib
import logging
import shutil
import sqlite3
from datetime import datetime
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path

    import aiosqlite

logger = logging.getLogger(__name__)

# Bump this when schema.sql has breaking changes that need tracking
SCHEMA_VERSION = 1

# Maximum number of backup files to keep
MAX_BACKUPS = 3


async def auto_migrate(
    conn: aiosqlite.Connection,
    schema_sql: str,
    db_path: Path | None = None,
) -> int:
    """Auto-migrate database to match schema.sql.

    Runs automatically on every app boot. This is the standard pattern for
    developer CLI tools and local-first apps (VS Code, Obsidian, sqlite-utils).

    Args:
        conn: Active aiosqlite connection
        schema_sql: Contents of schema.sql (source of truth)
        db_path: Path to database file (for backup creation)

    Returns:
        The current schema version after migration
    """
    cursor = await conn.execute("PRAGMA user_version")
    row = await cursor.fetchone()
    current_version = row[0] if row else 0

    # Create pristine in-memory DB from schema
    pristine = sqlite3.connect(":memory:")
    pristine.executescript(schema_sql)

    # Get table definitions
    pristine_tables = _get_tables_sync(pristine)
    actual_tables = await _get_tables_async(conn)

    changes_made = False
    changes_description: list[str] = []

    # Detect what needs to change
    new_tables = set(pristine_tables) - set(actual_tables)
    common_tables = set(pristine_tables) & set(actual_tables)
    changed_tables = [
        name
        for name in common_tables
        if _table_differs(pristine, name, pristine_tables[name], actual_tables[name])
    ]

    # Create backup before any destructive operations
    if (new_tables or changed_tables) and db_path and db_path.exists():
        _create_backup(db_path)

    # Create new tables
    for name in new_tables:
        logger.debug(f"Migration: Creating new table '{name}'")
        await conn.execute(pristine_tables[name])
        changes_description.append(f"created table '{name}'")
        changes_made = True

    # Recreate changed tables (preserves data in common columns)
    for name in changed_tables:
        logger.debug(f"Migration: Recreating table '{name}' (schema changed)")
        await _recreate_table(conn, pristine, name, pristine_tables[name])
        changes_description.append(f"updated table '{name}'")
        changes_made = True

    # Recreate indexes and triggers (idempotent)
    await _recreate_indexes_and_triggers(conn, schema_sql)

    # Update version
    if changes_made or current_version != SCHEMA_VERSION:
        await conn.execute(f"PRAGMA user_version = {SCHEMA_VERSION}")
        if changes_made:
            logger.info(
                f"Database migrated to schema v{SCHEMA_VERSION}: {', '.join(changes_description)}"
            )

    await conn.commit()
    pristine.close()

    return SCHEMA_VERSION


def _create_backup(db_path: Path) -> Path | None:
    """Create a timestamped backup before migration.

    Keeps only the last MAX_BACKUPS backup files to avoid disk bloat.
    """
    try:
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        backup_path = db_path.with_suffix(f".db.backup_{timestamp}")
        shutil.copy2(db_path, backup_path)
        logger.debug(f"Migration: Created backup at {backup_path}")

        # Clean up old backups (keep only MAX_BACKUPS most recent)
        backup_pattern = db_path.stem + ".db.backup_*"
        backups = sorted(db_path.parent.glob(backup_pattern), reverse=True)
        for old_backup in backups[MAX_BACKUPS:]:
            old_backup.unlink()
            logger.debug(f"Migration: Removed old backup {old_backup}")

        return backup_path
    except OSError as e:
        logger.warning(f"Migration: Could not create backup: {e}")
        return None


def _get_tables_sync(conn: sqlite3.Connection) -> dict[str, str]:
    """Get {table_name: CREATE SQL} from sync connection."""
    return {
        row[0]: row[1]
        for row in conn.execute(
            "SELECT name, sql FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'"
        )
        if row[1]
    }


async def _get_tables_async(conn: aiosqlite.Connection) -> dict[str, str]:
    """Get {table_name: CREATE SQL} from async connection."""
    cursor = await conn.execute(
        "SELECT name, sql FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'"
    )
    return {row[0]: row[1] for row in await cursor.fetchall() if row[1]}


def _table_differs(
    pristine: sqlite3.Connection, name: str, pristine_sql: str, actual_sql: str
) -> bool:
    """Check if table structure differs (normalized comparison)."""

    def normalize(sql: str) -> str:
        # Normalize whitespace and case for comparison
        return " ".join(sql.upper().split())

    return normalize(pristine_sql) != normalize(actual_sql)


async def _recreate_table(
    conn: aiosqlite.Connection,
    pristine: sqlite3.Connection,
    name: str,
    pristine_sql: str,
) -> None:
    """Recreate table using SQLite's 12-step procedure.

    This safely handles all schema changes including:
    - Column additions/removals
    - Type changes
    - Constraint changes
    - Default value changes

    Data is preserved for columns that exist in both old and new schemas.
    """
    # Find common columns (these will have their data preserved)
    pristine_cols = {r[1] for r in pristine.execute(f"PRAGMA table_info({name})")}
    cursor = await conn.execute(f"PRAGMA table_info({name})")
    actual_cols = {r[1] for r in await cursor.fetchall()}
    common_cols = pristine_cols & actual_cols

    if not common_cols:
        # No common columns - just recreate empty
        await conn.execute(f"DROP TABLE IF EXISTS {name}")
        await conn.execute(pristine_sql)
        return

    cols_csv = ", ".join(f'"{c}"' for c in common_cols)
    temp_name = f"_migrate_{name}"

    # Use 12-step procedure for safe recreation
    await conn.execute("PRAGMA foreign_keys=OFF")
    try:
        # Step 1: Create temp table with new schema
        # Handle various CREATE TABLE patterns
        temp_sql = pristine_sql
        for pattern in [
            f'CREATE TABLE IF NOT EXISTS "{name}"',
            f"CREATE TABLE IF NOT EXISTS {name}",
            f'CREATE TABLE "{name}"',
            f"CREATE TABLE {name}",
        ]:
            if pattern in temp_sql:
                temp_sql = temp_sql.replace(pattern, f'CREATE TABLE "{temp_name}"', 1)
                break

        await conn.execute(temp_sql)

        # Step 2: Copy data for common columns
        await conn.execute(
            f'INSERT INTO "{temp_name}" ({cols_csv}) SELECT {cols_csv} FROM "{name}"'
        )

        # Step 3: Drop old table
        await conn.execute(f'DROP TABLE "{name}"')

        # Step 4: Rename temp to original name
        await conn.execute(f'ALTER TABLE "{temp_name}" RENAME TO "{name}"')

    finally:
        await conn.execute("PRAGMA foreign_keys=ON")


async def _recreate_indexes_and_triggers(conn: aiosqlite.Connection, schema_sql: str) -> None:
    """Recreate indexes and triggers from schema.sql.

    These are idempotent due to IF NOT EXISTS in schema.sql.
    """
    for stmt in schema_sql.split(";"):
        stmt = stmt.strip()
        if not stmt:
            continue

        upper_stmt = stmt.upper()
        if upper_stmt.startswith("CREATE INDEX") or upper_stmt.startswith("CREATE TRIGGER"):
            with contextlib.suppress(Exception):
                await conn.execute(stmt)
