"""Tests for KnowledgeBase."""

from __future__ import annotations

import tempfile
from pathlib import Path

import pytest

from kagan.database import KnowledgeBase, StateManager


@pytest.fixture
async def knowledge_base():
    """Create knowledge base with temp database."""
    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = Path(tmpdir) / "test.db"
        manager = StateManager(db_path)
        await manager.initialize()
        kb = KnowledgeBase(manager.connection)
        yield kb
        await manager.close()


class TestKnowledgeBase:
    """Tests for KnowledgeBase."""

    async def test_add_and_search(self, knowledge_base: KnowledgeBase):
        """Test adding and searching entries."""
        await knowledge_base.add("t1", "Fixed login bug with OAuth", ["auth", "bugfix"])

        results = await knowledge_base.search("OAuth")
        assert len(results) == 1
        assert results[0].ticket_id == "t1"
        assert "auth" in results[0].tags

    async def test_search_no_results(self, knowledge_base: KnowledgeBase):
        """Test search with no matches."""
        results = await knowledge_base.search("nonexistent")
        assert results == []

    async def test_get_by_ticket(self, knowledge_base: KnowledgeBase):
        """Test getting entry by ticket ID."""
        await knowledge_base.add("t2", "Implemented caching", ["performance"])

        entry = await knowledge_base.get_by_ticket("t2")
        assert entry is not None
        assert entry.summary == "Implemented caching"

    async def test_get_by_ticket_not_found(self, knowledge_base: KnowledgeBase):
        """Test getting entry for non-existent ticket."""
        entry = await knowledge_base.get_by_ticket("nonexistent")
        assert entry is None

    async def test_add_without_tags(self, knowledge_base: KnowledgeBase):
        """Test adding entry without tags."""
        await knowledge_base.add("t3", "Simple fix")

        entry = await knowledge_base.get_by_ticket("t3")
        assert entry is not None
        assert entry.tags == []
