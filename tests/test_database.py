"""Tests for database operations."""

import tempfile
from pathlib import Path

import pytest

from kagan.database.manager import StateManager
from kagan.database.models import (
    TicketCreate,
    TicketPriority,
    TicketStatus,
    TicketUpdate,
)


@pytest.fixture
async def state_manager():
    """Create a temporary database for testing."""
    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = Path(tmpdir) / "test.db"
        manager = StateManager(db_path)
        await manager.initialize()
        yield manager
        await manager.close()


class TestStateManagerInitialization:
    """Tests for database initialization."""

    async def test_initialize_creates_db(self):
        """Test that initialization creates the database file."""
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = Path(tmpdir) / "subdir" / "test.db"
            manager = StateManager(db_path)
            await manager.initialize()

            assert db_path.exists()
            await manager.close()

    async def test_initialize_idempotent(self, state_manager: StateManager):
        """Test that initialization can be called multiple times."""
        await state_manager.initialize()
        await state_manager.initialize()
        # Should not raise


class TestTicketCRUD:
    """Tests for ticket CRUD operations."""

    async def test_create_ticket(self, state_manager: StateManager):
        """Test creating a ticket."""
        create = TicketCreate(
            title="Test ticket",
            description="Test description",
            priority=TicketPriority.HIGH,
        )
        ticket = await state_manager.create_ticket(create)

        assert ticket.title == "Test ticket"
        assert ticket.description == "Test description"
        assert ticket.priority == TicketPriority.HIGH
        assert ticket.status == TicketStatus.BACKLOG
        assert len(ticket.id) == 8

    async def test_get_ticket(self, state_manager: StateManager):
        """Test retrieving a ticket by ID."""
        create = TicketCreate(title="Get me")
        created = await state_manager.create_ticket(create)

        ticket = await state_manager.get_ticket(created.id)

        assert ticket is not None
        assert ticket.id == created.id
        assert ticket.title == "Get me"

    async def test_get_ticket_not_found(self, state_manager: StateManager):
        """Test retrieving a non-existent ticket."""
        ticket = await state_manager.get_ticket("nonexistent")
        assert ticket is None

    async def test_get_all_tickets(self, state_manager: StateManager):
        """Test retrieving all tickets."""
        await state_manager.create_ticket(TicketCreate(title="Ticket 1"))
        await state_manager.create_ticket(TicketCreate(title="Ticket 2"))
        await state_manager.create_ticket(TicketCreate(title="Ticket 3"))

        tickets = await state_manager.get_all_tickets()

        assert len(tickets) == 3
        titles = [t.title for t in tickets]
        assert "Ticket 1" in titles
        assert "Ticket 2" in titles
        assert "Ticket 3" in titles

    async def test_get_tickets_by_status(self, state_manager: StateManager):
        """Test filtering tickets by status."""
        await state_manager.create_ticket(
            TicketCreate(title="Backlog 1", status=TicketStatus.BACKLOG)
        )
        await state_manager.create_ticket(
            TicketCreate(title="Backlog 2", status=TicketStatus.BACKLOG)
        )
        await state_manager.create_ticket(
            TicketCreate(title="In Progress", status=TicketStatus.IN_PROGRESS)
        )

        backlog = await state_manager.get_tickets_by_status(TicketStatus.BACKLOG)
        in_progress = await state_manager.get_tickets_by_status(TicketStatus.IN_PROGRESS)
        review = await state_manager.get_tickets_by_status(TicketStatus.REVIEW)

        assert len(backlog) == 2
        assert len(in_progress) == 1
        assert len(review) == 0

    async def test_update_ticket(self, state_manager: StateManager):
        """Test updating a ticket."""
        create = TicketCreate(title="Original")
        ticket = await state_manager.create_ticket(create)

        update = TicketUpdate(title="Updated", priority=TicketPriority.HIGH)
        updated = await state_manager.update_ticket(ticket.id, update)

        assert updated is not None
        assert updated.title == "Updated"
        assert updated.priority == TicketPriority.HIGH

    async def test_update_ticket_partial(self, state_manager: StateManager):
        """Test partial update preserves other fields."""
        create = TicketCreate(
            title="Original",
            description="Keep this",
            priority=TicketPriority.HIGH,
        )
        ticket = await state_manager.create_ticket(create)

        update = TicketUpdate(title="New title")
        updated = await state_manager.update_ticket(ticket.id, update)

        assert updated is not None
        assert updated.title == "New title"
        assert updated.description == "Keep this"
        assert updated.priority == TicketPriority.HIGH

    async def test_delete_ticket(self, state_manager: StateManager):
        """Test deleting a ticket."""
        create = TicketCreate(title="Delete me")
        ticket = await state_manager.create_ticket(create)

        result = await state_manager.delete_ticket(ticket.id)

        assert result is True
        assert await state_manager.get_ticket(ticket.id) is None

    async def test_delete_ticket_not_found(self, state_manager: StateManager):
        """Test deleting a non-existent ticket."""
        result = await state_manager.delete_ticket("nonexistent")
        assert result is False

    async def test_move_ticket(self, state_manager: StateManager):
        """Test moving a ticket to a new status."""
        create = TicketCreate(title="Move me")
        ticket = await state_manager.create_ticket(create)

        moved = await state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)

        assert moved is not None
        assert moved.status == TicketStatus.IN_PROGRESS


class TestTicketOrdering:
    """Tests for ticket ordering."""

    async def test_tickets_ordered_by_priority(self, state_manager: StateManager):
        """Test that tickets are ordered by priority descending."""
        await state_manager.create_ticket(TicketCreate(title="Low", priority=TicketPriority.LOW))
        await state_manager.create_ticket(TicketCreate(title="High", priority=TicketPriority.HIGH))
        await state_manager.create_ticket(
            TicketCreate(title="Medium", priority=TicketPriority.MEDIUM)
        )

        tickets = await state_manager.get_all_tickets()

        # High priority first
        assert tickets[0].title == "High"
        assert tickets[1].title == "Medium"
        assert tickets[2].title == "Low"


class TestTicketCounts:
    """Tests for ticket count operations."""

    async def test_get_ticket_counts_empty(self, state_manager: StateManager):
        """Test counts with no tickets."""
        counts = await state_manager.get_ticket_counts()

        assert counts[TicketStatus.BACKLOG] == 0
        assert counts[TicketStatus.IN_PROGRESS] == 0
        assert counts[TicketStatus.REVIEW] == 0
        assert counts[TicketStatus.DONE] == 0

    async def test_get_ticket_counts(self, state_manager: StateManager):
        """Test counts with tickets."""
        await state_manager.create_ticket(TicketCreate(title="B1", status=TicketStatus.BACKLOG))
        await state_manager.create_ticket(TicketCreate(title="B2", status=TicketStatus.BACKLOG))
        await state_manager.create_ticket(TicketCreate(title="IP", status=TicketStatus.IN_PROGRESS))
        await state_manager.create_ticket(TicketCreate(title="D1", status=TicketStatus.DONE))
        await state_manager.create_ticket(TicketCreate(title="D2", status=TicketStatus.DONE))
        await state_manager.create_ticket(TicketCreate(title="D3", status=TicketStatus.DONE))

        counts = await state_manager.get_ticket_counts()

        assert counts[TicketStatus.BACKLOG] == 2
        assert counts[TicketStatus.IN_PROGRESS] == 1
        assert counts[TicketStatus.REVIEW] == 0
        assert counts[TicketStatus.DONE] == 3


class TestScratchpads:
    """Tests for scratchpad operations."""

    async def test_get_scratchpad_empty(self, state_manager: StateManager):
        """Returns empty string for nonexistent scratchpad."""
        result = await state_manager.get_scratchpad("nonexistent")
        assert result == ""

    async def test_update_and_get_scratchpad(self, state_manager: StateManager):
        """Can create and retrieve scratchpad."""
        ticket = await state_manager.create_ticket(TicketCreate(title="Test"))
        await state_manager.update_scratchpad(ticket.id, "Progress notes")

        result = await state_manager.get_scratchpad(ticket.id)
        assert result == "Progress notes"

    async def test_update_scratchpad_overwrites(self, state_manager: StateManager):
        """Updates overwrite existing content."""
        ticket = await state_manager.create_ticket(TicketCreate(title="Test"))
        await state_manager.update_scratchpad(ticket.id, "First")
        await state_manager.update_scratchpad(ticket.id, "Second")

        result = await state_manager.get_scratchpad(ticket.id)
        assert result == "Second"

    async def test_delete_scratchpad(self, state_manager: StateManager):
        """Can delete scratchpad."""
        ticket = await state_manager.create_ticket(TicketCreate(title="Test"))
        await state_manager.update_scratchpad(ticket.id, "Content")
        await state_manager.delete_scratchpad(ticket.id)

        result = await state_manager.get_scratchpad(ticket.id)
        assert result == ""

    async def test_scratchpad_size_limit(self, state_manager: StateManager):
        """Scratchpad content is truncated at 50000 chars."""
        ticket = await state_manager.create_ticket(TicketCreate(title="Test"))
        long_content = "x" * 60000
        await state_manager.update_scratchpad(ticket.id, long_content)

        result = await state_manager.get_scratchpad(ticket.id)
        assert len(result) == 50000
