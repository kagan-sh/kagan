from __future__ import annotations

import pytest
from sqlalchemy.exc import IntegrityError

from kagan.core.adapters.db.engine import create_db_engine


async def test_sqlite_foreign_keys_are_enforced() -> None:
    engine = await create_db_engine(":memory:")
    try:
        async with engine.begin() as conn:
            await conn.exec_driver_sql("CREATE TABLE parent (id INTEGER PRIMARY KEY)")
            await conn.exec_driver_sql(
                "CREATE TABLE child (id INTEGER PRIMARY KEY, "
                "parent_id INTEGER REFERENCES parent(id))"
            )

            with pytest.raises(IntegrityError):
                await conn.exec_driver_sql("INSERT INTO child (id, parent_id) VALUES (1, 999)")
    finally:
        await engine.dispose()
