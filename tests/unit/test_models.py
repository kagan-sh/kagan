"""Tests for Pydantic models."""

from __future__ import annotations

import pytest
from pydantic import ValidationError

from kagan.database.models import (
    Ticket,
    TicketPriority,
    TicketStatus,
)

pytestmark = pytest.mark.unit


class TestTicketStatus:
    """Tests for TicketStatus enum."""

    @pytest.mark.parametrize(
        ("current", "expected"),
        [
            (TicketStatus.BACKLOG, TicketStatus.IN_PROGRESS),
            (TicketStatus.IN_PROGRESS, TicketStatus.REVIEW),
            (TicketStatus.REVIEW, TicketStatus.DONE),
            (TicketStatus.DONE, None),
        ],
    )
    def test_next_status(self, current: TicketStatus, expected: TicketStatus | None) -> None:
        """Test status progression."""
        assert TicketStatus.next_status(current) == expected

    @pytest.mark.parametrize(
        ("current", "expected"),
        [
            (TicketStatus.BACKLOG, None),
            (TicketStatus.IN_PROGRESS, TicketStatus.BACKLOG),
            (TicketStatus.REVIEW, TicketStatus.IN_PROGRESS),
            (TicketStatus.DONE, TicketStatus.REVIEW),
        ],
    )
    def test_prev_status(self, current: TicketStatus, expected: TicketStatus | None) -> None:
        """Test status regression."""
        assert TicketStatus.prev_status(current) == expected


class TestTicket:
    """Tests for Ticket model."""

    def test_ticket_short_id(self):
        """Test short ID property."""
        ticket = Ticket(id="abc123456789", title="Test")
        assert ticket.short_id == "abc12345"

    @pytest.mark.parametrize(
        ("priority", "expected_label"),
        [
            (TicketPriority.LOW, "LOW"),
            (TicketPriority.MEDIUM, "MED"),
            (TicketPriority.HIGH, "HIGH"),
        ],
    )
    def test_ticket_priority_label(self, priority: TicketPriority, expected_label: str) -> None:
        """Test priority label property."""
        ticket = Ticket(title="Test", priority=priority)
        assert ticket.priority_label == expected_label

    @pytest.mark.parametrize("title", ["", "x" * 201])
    def test_ticket_title_validation(self, title: str):
        """Test title validation rejects empty or too long titles."""
        with pytest.raises(ValidationError):
            Ticket(title=title)
