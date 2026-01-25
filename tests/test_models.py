"""Tests for Pydantic models."""

import pytest
from pydantic import ValidationError

from kagan.database.models import (
    Ticket,
    TicketCreate,
    TicketPriority,
    TicketStatus,
    TicketUpdate,
)


class TestTicketStatus:
    """Tests for TicketStatus enum."""

    def test_next_status(self):
        """Test status progression."""
        assert TicketStatus.next_status(TicketStatus.BACKLOG) == TicketStatus.IN_PROGRESS
        assert TicketStatus.next_status(TicketStatus.IN_PROGRESS) == TicketStatus.REVIEW
        assert TicketStatus.next_status(TicketStatus.REVIEW) == TicketStatus.DONE
        assert TicketStatus.next_status(TicketStatus.DONE) is None

    def test_prev_status(self):
        """Test status regression."""
        assert TicketStatus.prev_status(TicketStatus.BACKLOG) is None
        assert TicketStatus.prev_status(TicketStatus.IN_PROGRESS) == TicketStatus.BACKLOG
        assert TicketStatus.prev_status(TicketStatus.REVIEW) == TicketStatus.IN_PROGRESS
        assert TicketStatus.prev_status(TicketStatus.DONE) == TicketStatus.REVIEW


class TestTicket:
    """Tests for Ticket model."""

    def test_create_ticket_minimal(self):
        """Test creating a ticket with minimal fields."""
        ticket = Ticket(title="Test ticket")
        assert ticket.title == "Test ticket"
        assert ticket.description == ""
        assert ticket.status == TicketStatus.BACKLOG
        assert ticket.priority == TicketPriority.MEDIUM
        assert ticket.parent_id is None
        assert len(ticket.id) == 8

    def test_create_ticket_full(self):
        """Test creating a ticket with all fields."""
        ticket = Ticket(
            id="abc12345",
            title="Full ticket",
            description="A detailed description",
            status=TicketStatus.IN_PROGRESS,
            priority=TicketPriority.HIGH,
            parent_id="parent123",
        )
        assert ticket.id == "abc12345"
        assert ticket.title == "Full ticket"
        assert ticket.description == "A detailed description"
        assert ticket.status == TicketStatus.IN_PROGRESS
        assert ticket.priority == TicketPriority.HIGH
        assert ticket.parent_id == "parent123"

    def test_ticket_short_id(self):
        """Test short ID property."""
        ticket = Ticket(id="abc123456789", title="Test")
        assert ticket.short_id == "abc12345"

    def test_ticket_priority_label(self):
        """Test priority label property."""
        ticket_low = Ticket(title="Test", priority=TicketPriority.LOW)
        ticket_med = Ticket(title="Test", priority=TicketPriority.MEDIUM)
        ticket_high = Ticket(title="Test", priority=TicketPriority.HIGH)

        assert ticket_low.priority_label == "LOW"
        assert ticket_med.priority_label == "MED"
        assert ticket_high.priority_label == "HIGH"

    def test_ticket_title_validation(self):
        """Test title validation."""
        with pytest.raises(ValidationError):
            Ticket(title="")

        with pytest.raises(ValidationError):
            Ticket(title="x" * 201)


class TestTicketCreate:
    """Tests for TicketCreate model."""

    def test_create_minimal(self):
        """Test creating with minimal fields."""
        create = TicketCreate(title="New ticket")
        assert create.title == "New ticket"
        assert create.description == ""
        assert create.priority == TicketPriority.MEDIUM
        assert create.status == TicketStatus.BACKLOG

    def test_create_full(self):
        """Test creating with all fields."""
        create = TicketCreate(
            title="Full ticket",
            description="Description",
            priority=TicketPriority.HIGH,
            status=TicketStatus.IN_PROGRESS,
        )
        assert create.title == "Full ticket"
        assert create.description == "Description"
        assert create.priority == TicketPriority.HIGH
        assert create.status == TicketStatus.IN_PROGRESS


class TestTicketUpdate:
    """Tests for TicketUpdate model."""

    def test_update_empty(self):
        """Test update with no fields."""
        update = TicketUpdate()
        assert update.title is None
        assert update.description is None
        assert update.priority is None
        assert update.status is None

    def test_update_partial(self):
        """Test partial update."""
        update = TicketUpdate(title="New title", priority=TicketPriority.HIGH)
        assert update.title == "New title"
        assert update.description is None
        assert update.priority == TicketPriority.HIGH
        assert update.status is None
