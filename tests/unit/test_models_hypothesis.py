"""Property-based tests for Pydantic models using Hypothesis."""

from __future__ import annotations

import pytest
from hypothesis import given
from pydantic import ValidationError

from kagan.database.models import Ticket, TicketPriority, TicketStatus, TicketType
from tests.strategies import (
    empty_titles,
    oversized_titles,
    priorities,
    statuses,
    ticket_types,
    tickets,
    valid_ticket_titles,
)

pytestmark = pytest.mark.unit


class TestTicketProperties:
    """Property-based tests for Ticket model."""

    @given(valid_ticket_titles)
    def test_valid_titles_create_tickets(self, title: str) -> None:
        """Any valid title (1-200 chars, non-empty strip) creates a ticket."""
        ticket = Ticket(title=title)
        assert ticket.title == title
        assert len(ticket.title) >= 1
        assert len(ticket.title) <= 200

    @given(empty_titles)
    def test_empty_titles_rejected(self, title: str) -> None:
        """Empty titles are rejected by validation."""
        with pytest.raises(ValidationError):
            Ticket(title=title)

    @given(oversized_titles)
    def test_oversized_titles_rejected(self, title: str) -> None:
        """Titles over 200 chars are rejected by validation."""
        with pytest.raises(ValidationError):
            Ticket(title=title)

    @given(tickets())
    def test_short_id_is_prefix_of_full_id(self, ticket: Ticket) -> None:
        """short_id is always the first 8 chars of full id."""
        assert ticket.short_id == ticket.id[:8]
        assert len(ticket.short_id) == 8

    @given(priorities)
    def test_priority_label_exists(self, priority: TicketPriority) -> None:
        """Every priority has a non-empty label."""
        ticket = Ticket(title="Test", priority=priority)
        assert ticket.priority_label in ("LOW", "MED", "HIGH")
        assert len(ticket.priority_label) > 0

    @given(priorities)
    def test_priority_css_class_exists(self, priority: TicketPriority) -> None:
        """Every priority has a CSS class."""
        assert priority.css_class in ("low", "medium", "high")


class TestStatusRoundtrip:
    """Property-based tests for status transitions."""

    @given(statuses)
    def test_status_roundtrip(self, status: TicketStatus) -> None:
        """Status survives serialization roundtrip."""
        ticket = Ticket(title="Test", status=status)
        # Simulate database roundtrip via model_dump/parse
        data = ticket.model_dump()
        restored = Ticket.model_validate(data)
        assert restored.status == status

    @given(statuses)
    def test_next_then_prev_returns_original_for_middle_statuses(
        self, status: TicketStatus
    ) -> None:
        """next_status then prev_status returns original (except at boundaries)."""
        next_status = TicketStatus.next_status(status)
        if next_status is not None:
            prev_of_next = TicketStatus.prev_status(next_status)
            assert prev_of_next == status

    @given(statuses)
    def test_prev_then_next_returns_original_for_middle_statuses(
        self, status: TicketStatus
    ) -> None:
        """prev_status then next_status returns original (except at boundaries)."""
        prev_status = TicketStatus.prev_status(status)
        if prev_status is not None:
            next_of_prev = TicketStatus.next_status(prev_status)
            assert next_of_prev == status


class TestEnumCoercion:
    """Property-based tests for enum coercion from strings."""

    @given(statuses)
    def test_status_coercion_from_string(self, status: TicketStatus) -> None:
        """Status can be created from its string value."""
        ticket = Ticket(title="Test", status=status.value)
        assert ticket.status == status

    @given(priorities)
    def test_priority_coercion_from_int(self, priority: TicketPriority) -> None:
        """Priority can be created from its int value."""
        ticket = Ticket(title="Test", priority=priority.value)
        assert ticket.priority == priority

    @given(ticket_types)
    def test_ticket_type_coercion_from_string(self, tt: TicketType) -> None:
        """TicketType can be created from its string value."""
        ticket = Ticket(title="Test", ticket_type=tt.value)
        assert ticket.ticket_type == tt


class TestStatusBoundaries:
    """Explicit boundary tests for status transitions (migrated from test_models.py)."""

    def test_next_status_boundaries(self) -> None:
        """Status progression from BACKLOG to DONE, then None."""
        assert TicketStatus.next_status(TicketStatus.BACKLOG) == TicketStatus.IN_PROGRESS
        assert TicketStatus.next_status(TicketStatus.IN_PROGRESS) == TicketStatus.REVIEW
        assert TicketStatus.next_status(TicketStatus.REVIEW) == TicketStatus.DONE
        assert TicketStatus.next_status(TicketStatus.DONE) is None

    def test_prev_status_boundaries(self) -> None:
        """Status regression from DONE to BACKLOG, then None."""
        assert TicketStatus.prev_status(TicketStatus.BACKLOG) is None
        assert TicketStatus.prev_status(TicketStatus.IN_PROGRESS) == TicketStatus.BACKLOG
        assert TicketStatus.prev_status(TicketStatus.REVIEW) == TicketStatus.IN_PROGRESS
        assert TicketStatus.prev_status(TicketStatus.DONE) == TicketStatus.REVIEW
