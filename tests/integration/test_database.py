"""Tests for database operations - edge cases not covered by property tests.

See test_database_hypothesis.py for comprehensive property-based tests.
"""

from __future__ import annotations

import pytest

from kagan.database.manager import StateManager
from kagan.database.models import (
    Ticket,
    TicketPriority,
    TicketStatus,
)

pytestmark = pytest.mark.integration


class TestStateManagerInitialization:
    """Tests for database initialization."""

    async def test_initialize_creates_db(self, tmp_path):
        """Test that initialization creates the database file."""
        db_path = tmp_path / "test.db"
        manager = StateManager(db_path)
        await manager.initialize()
        assert db_path.exists()
        await manager.close()

    async def test_initialize_idempotent(self, state_manager):
        """Test that initialization can be called multiple times."""
        await state_manager.initialize()
        await state_manager.initialize()


class TestTicketCRUD:
    """Tests for ticket CRUD operations (edge cases not covered by property tests)."""

    async def test_get_ticket_not_found(self, state_manager):
        """Test retrieving a non-existent ticket."""
        ticket = await state_manager.get_ticket("nonexistent")
        assert ticket is None

    async def test_get_all_tickets(self, state_manager):
        """Test retrieving all tickets."""
        await state_manager.create_ticket(Ticket.create(title="Ticket 1"))
        await state_manager.create_ticket(Ticket.create(title="Ticket 2"))
        await state_manager.create_ticket(Ticket.create(title="Ticket 3"))

        tickets = await state_manager.get_all_tickets()

        assert len(tickets) == 3
        titles = [t.title for t in tickets]
        assert "Ticket 1" in titles
        assert "Ticket 2" in titles
        assert "Ticket 3" in titles


class TestTicketOrdering:
    """Tests for ticket ordering."""

    async def test_tickets_ordered_by_priority(self, state_manager):
        """Test that tickets are ordered by priority descending."""
        await state_manager.create_ticket(Ticket.create(title="Low", priority=TicketPriority.LOW))
        await state_manager.create_ticket(Ticket.create(title="High", priority=TicketPriority.HIGH))
        await state_manager.create_ticket(
            Ticket.create(title="Medium", priority=TicketPriority.MEDIUM)
        )

        tickets = await state_manager.get_all_tickets()

        assert tickets[0].title == "High"
        assert tickets[1].title == "Medium"
        assert tickets[2].title == "Low"


class TestTicketCounts:
    """Tests for ticket count operations (edge cases)."""

    async def test_get_ticket_counts_empty(self, state_manager):
        """Test counts with no tickets."""
        counts = await state_manager.get_ticket_counts()

        assert counts[TicketStatus.BACKLOG] == 0
        assert counts[TicketStatus.IN_PROGRESS] == 0
        assert counts[TicketStatus.REVIEW] == 0
        assert counts[TicketStatus.DONE] == 0
