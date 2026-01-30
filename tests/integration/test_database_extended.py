"""Extended database tests - scratchpads, new fields, ticket types."""

from __future__ import annotations

import pytest

from kagan.database.models import (
    TicketCreate,
    TicketType,
    TicketUpdate,
)

pytestmark = pytest.mark.integration


class TestScratchpads:
    """Tests for scratchpad operations."""

    async def test_get_scratchpad_empty(self, state_manager):
        """Returns empty string for nonexistent scratchpad."""
        result = await state_manager.get_scratchpad("nonexistent")
        assert result == ""

    async def test_update_and_get_scratchpad(self, state_manager):
        """Can create and retrieve scratchpad."""
        ticket = await state_manager.create_ticket(TicketCreate(title="Test"))
        await state_manager.update_scratchpad(ticket.id, "Progress notes")

        result = await state_manager.get_scratchpad(ticket.id)
        assert result == "Progress notes"

    async def test_update_scratchpad_overwrites(self, state_manager):
        """Updates overwrite existing content."""
        ticket = await state_manager.create_ticket(TicketCreate(title="Test"))
        await state_manager.update_scratchpad(ticket.id, "First")
        await state_manager.update_scratchpad(ticket.id, "Second")

        result = await state_manager.get_scratchpad(ticket.id)
        assert result == "Second"

    async def test_delete_scratchpad(self, state_manager):
        """Can delete scratchpad."""
        ticket = await state_manager.create_ticket(TicketCreate(title="Test"))
        await state_manager.update_scratchpad(ticket.id, "Content")
        await state_manager.delete_scratchpad(ticket.id)

        result = await state_manager.get_scratchpad(ticket.id)
        assert result == ""

    async def test_scratchpad_size_limit(self, state_manager):
        """Scratchpad content is truncated at 50000 chars."""
        ticket = await state_manager.create_ticket(TicketCreate(title="Test"))
        long_content = "x" * 60000
        await state_manager.update_scratchpad(ticket.id, long_content)

        result = await state_manager.get_scratchpad(ticket.id)
        assert len(result) == 50000


class TestTicketNewFields:
    """Tests for new ticket fields."""

    async def test_create_ticket_with_new_fields(self, state_manager):
        """Test creating a ticket with new fields."""
        create = TicketCreate(
            title="Test ticket",
            acceptance_criteria=["All tests pass"],
            checks_passed=True,
            session_active=True,
        )
        ticket = await state_manager.create_ticket(create)

        assert ticket.acceptance_criteria == ["All tests pass"]
        assert ticket.checks_passed is True
        assert ticket.session_active is True
        assert ticket.review_summary is None

    async def test_get_ticket_with_new_fields(self, state_manager):
        """Test retrieving a ticket with new fields."""
        create = TicketCreate(
            title="Test",
            acceptance_criteria=["Criteria"],
            review_summary="Looks good",
            checks_passed=True,
            session_active=False,
        )
        created = await state_manager.create_ticket(create)

        ticket = await state_manager.get_ticket(created.id)

        assert ticket is not None
        assert ticket.acceptance_criteria == ["Criteria"]
        assert ticket.review_summary == "Looks good"
        assert ticket.checks_passed is True
        assert ticket.session_active is False

    async def test_update_ticket_new_fields(self, state_manager):
        """Test updating new fields on a ticket."""
        create = TicketCreate(title="Original")
        ticket = await state_manager.create_ticket(create)

        update = TicketUpdate(
            acceptance_criteria=["New criteria"],
            checks_passed=True,
        )
        updated = await state_manager.update_ticket(ticket.id, update)

        assert updated is not None
        assert updated.acceptance_criteria == ["New criteria"]
        assert updated.checks_passed is True
        assert updated.session_active is False

    async def test_mark_session_active(self, state_manager):
        """Test marking a ticket's session as active."""
        create = TicketCreate(title="Test")
        ticket = await state_manager.create_ticket(create)

        assert ticket.session_active is False

        updated = await state_manager.mark_session_active(ticket.id, True)
        assert updated is not None
        assert updated.session_active is True

        updated = await state_manager.mark_session_active(ticket.id, False)
        assert updated is not None
        assert updated.session_active is False

    async def test_set_review_summary(self, state_manager):
        """Test setting review summary for a ticket."""
        create = TicketCreate(title="Test")
        ticket = await state_manager.create_ticket(create)

        assert ticket.review_summary is None

        updated = await state_manager.set_review_summary(ticket.id, "Great work!", True)
        assert updated is not None
        assert updated.review_summary == "Great work!"
        assert updated.checks_passed is True

        updated = await state_manager.set_review_summary(ticket.id, "Updated review", False)
        assert updated is not None
        assert updated.review_summary == "Updated review"
        assert updated.checks_passed is False

    async def test_new_fields_defaults(self, state_manager):
        """Test that new fields have correct defaults."""
        create = TicketCreate(title="Test")
        ticket = await state_manager.create_ticket(create)

        assert ticket.acceptance_criteria == []
        assert ticket.review_summary is None
        assert ticket.checks_passed is None
        assert ticket.session_active is False

    async def test_update_partial_new_fields(self, state_manager):
        """Test partial update preserves other new fields."""
        create = TicketCreate(
            title="Test",
            acceptance_criteria=["Original"],
            checks_passed=True,
        )
        ticket = await state_manager.create_ticket(create)

        update = TicketUpdate(review_summary="Looks good")
        updated = await state_manager.update_ticket(ticket.id, update)

        assert updated is not None
        assert updated.acceptance_criteria == ["Original"]
        assert updated.review_summary == "Looks good"
        assert updated.checks_passed is True


class TestTicketType:
    """Tests for ticket_type field."""

    async def test_default_ticket_type_is_pair(self, state_manager):
        """Test that default ticket_type is PAIR."""
        create = TicketCreate(title="Test")
        ticket = await state_manager.create_ticket(create)

        assert ticket.ticket_type == TicketType.PAIR

    async def test_create_auto_ticket(self, state_manager):
        """Test creating an AUTO ticket."""
        create = TicketCreate(title="Auto ticket", ticket_type=TicketType.AUTO)
        ticket = await state_manager.create_ticket(create)

        assert ticket.ticket_type == TicketType.AUTO

    async def test_create_pair_ticket(self, state_manager):
        """Test creating a PAIR ticket explicitly."""
        create = TicketCreate(title="Pair ticket", ticket_type=TicketType.PAIR)
        ticket = await state_manager.create_ticket(create)

        assert ticket.ticket_type == TicketType.PAIR

    async def test_get_ticket_preserves_type(self, state_manager):
        """Test that ticket type is preserved on get."""
        create = TicketCreate(title="Auto ticket", ticket_type=TicketType.AUTO)
        created = await state_manager.create_ticket(create)

        ticket = await state_manager.get_ticket(created.id)

        assert ticket is not None
        assert ticket.ticket_type == TicketType.AUTO

    async def test_update_ticket_type(self, state_manager):
        """Test updating ticket type."""
        create = TicketCreate(title="Test", ticket_type=TicketType.PAIR)
        ticket = await state_manager.create_ticket(create)
        assert ticket.ticket_type == TicketType.PAIR

        update = TicketUpdate(ticket_type=TicketType.AUTO)
        updated = await state_manager.update_ticket(ticket.id, update)

        assert updated is not None
        assert updated.ticket_type == TicketType.AUTO

    async def test_toggle_ticket_type(self, state_manager):
        """Test toggling ticket type back and forth."""
        create = TicketCreate(title="Test", ticket_type=TicketType.PAIR)
        ticket = await state_manager.create_ticket(create)

        updated = await state_manager.update_ticket(
            ticket.id, TicketUpdate(ticket_type=TicketType.AUTO)
        )
        assert updated is not None
        assert updated.ticket_type == TicketType.AUTO

        updated = await state_manager.update_ticket(
            ticket.id, TicketUpdate(ticket_type=TicketType.PAIR)
        )
        assert updated is not None
        assert updated.ticket_type == TicketType.PAIR
