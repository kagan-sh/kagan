"""Pytest fixtures for Kagan tests."""

from __future__ import annotations

import tempfile
from pathlib import Path

import pytest

from kagan.app import KaganApp
from kagan.database.manager import StateManager


@pytest.fixture
async def state_manager():
    """Create a temporary database for testing."""
    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = Path(tmpdir) / "test.db"
        manager = StateManager(db_path)
        await manager.initialize()
        yield manager
        await manager.close()


@pytest.fixture
def app() -> KaganApp:
    """Create app with in-memory database."""
    return KaganApp(db_path=":memory:")
