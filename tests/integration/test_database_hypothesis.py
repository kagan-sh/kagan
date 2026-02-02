"""Property-based tests for database operations using Hypothesis."""

from __future__ import annotations

import pytest
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from kagan.database.models import Ticket, TicketPriority, TicketStatus, TicketType
from kagan.limits import SCRATCHPAD_LIMIT
from tests.strategies import (
    plain_text,
    priorities,
    statuses,
    ticket_types,
    tickets,
    valid_ticket_titles,
)

pytestmark = pytest.mark.integration

# Shared settings for integration tests with fixtures
# - Suppress function_scoped_fixture: Each test creates independent tickets with unique IDs,
#   so reusing the database connection across hypothesis iterations is safe.
# - max_examples=20: Limit iterations for integration tests to avoid slow test runs.
integration_settings = settings(
    max_examples=20,
    suppress_health_check=[HealthCheck.function_scoped_fixture],
)

# Settings for slower tests (oversized data generation)
slow_integration_settings = settings(
    max_examples=10,
    suppress_health_check=[HealthCheck.function_scoped_fixture],
)

# =============================================================================
# Scratchpad Strategies
# =============================================================================

# Scratchpad content within limits
scratchpad_content = st.text(
    alphabet=st.characters(
        blacklist_categories=("Cs",),  # No surrogates
        blacklist_characters="\x00",  # No null bytes
    ),
    min_size=0,
    max_size=1000,
)

# Strategy for generating oversized content using a base + repetition
# Hypothesis cannot generate text > ~10000 chars directly
base_content_for_oversized = st.text(
    alphabet=st.characters(
        blacklist_categories=("Cs",),
        blacklist_characters="\x00",
    ),
    min_size=100,
    max_size=500,
)


@st.composite
def oversized_scratchpad(draw: st.DrawFn) -> str:
    """Generate content that exceeds SCRATCHPAD_LIMIT (50000 chars)."""
    base = draw(base_content_for_oversized)
    # Repeat the base to exceed the limit
    repeat_count = (SCRATCHPAD_LIMIT // len(base)) + 10
    return base * repeat_count


# =============================================================================
# Ticket CRUD Properties
# =============================================================================


class TestTicketRoundtrip:
    """Property-based tests for ticket create-read roundtrip."""

    @integration_settings
    @given(tickets())
    async def test_ticket_roundtrip_preserves_all_fields(
        self, state_manager, ticket: Ticket
    ) -> None:
        """Creating and retrieving a ticket preserves all core fields."""
        created = await state_manager.create_ticket(ticket)

        retrieved = await state_manager.get_ticket(created.id)

        assert retrieved is not None
        assert retrieved.id == created.id
        assert retrieved.title == ticket.title
        assert retrieved.status == ticket.status
        assert retrieved.priority == ticket.priority
        assert retrieved.ticket_type == ticket.ticket_type

    @integration_settings
    @given(valid_ticket_titles)
    async def test_any_valid_title_persists(self, state_manager, title: str) -> None:
        """Any valid title (1-200 chars) can be stored and retrieved."""
        ticket = Ticket.create(title=title)
        created = await state_manager.create_ticket(ticket)

        retrieved = await state_manager.get_ticket(created.id)

        assert retrieved is not None
        assert retrieved.title == title

    @integration_settings
    @given(statuses)
    async def test_any_status_persists(self, state_manager, status: TicketStatus) -> None:
        """Any valid status can be stored and retrieved."""
        ticket = Ticket.create(title="Status test", status=status)
        created = await state_manager.create_ticket(ticket)

        retrieved = await state_manager.get_ticket(created.id)

        assert retrieved is not None
        assert retrieved.status == status

    @integration_settings
    @given(priorities)
    async def test_any_priority_persists(self, state_manager, priority: TicketPriority) -> None:
        """Any valid priority can be stored and retrieved."""
        ticket = Ticket.create(title="Priority test", priority=priority)
        created = await state_manager.create_ticket(ticket)

        retrieved = await state_manager.get_ticket(created.id)

        assert retrieved is not None
        assert retrieved.priority == priority

    @integration_settings
    @given(ticket_types)
    async def test_any_ticket_type_persists(self, state_manager, ticket_type: TicketType) -> None:
        """Any valid ticket type can be stored and retrieved."""
        ticket = Ticket.create(title="Type test", ticket_type=ticket_type)
        created = await state_manager.create_ticket(ticket)

        retrieved = await state_manager.get_ticket(created.id)

        assert retrieved is not None
        assert retrieved.ticket_type == ticket_type


class TestTicketUpdate:
    """Property-based tests for ticket update operations."""

    @integration_settings
    @given(valid_ticket_titles, valid_ticket_titles)
    async def test_title_update_roundtrip(
        self, state_manager, original_title: str, new_title: str
    ) -> None:
        """Updating title preserves the new value."""
        ticket = Ticket.create(title=original_title)
        created = await state_manager.create_ticket(ticket)

        updated = await state_manager.update_ticket(created.id, title=new_title)

        assert updated is not None
        assert updated.title == new_title

    @integration_settings
    @given(statuses, statuses)
    async def test_status_update_roundtrip(
        self, state_manager, initial_status: TicketStatus, new_status: TicketStatus
    ) -> None:
        """Updating status works for any valid status transition."""
        ticket = Ticket.create(title="Status update test", status=initial_status)
        created = await state_manager.create_ticket(ticket)

        updated = await state_manager.update_ticket(created.id, status=new_status)

        assert updated is not None
        assert updated.status == new_status

    @integration_settings
    @given(tickets(), priorities)
    async def test_partial_update_preserves_other_fields(
        self, state_manager, ticket: Ticket, new_priority: TicketPriority
    ) -> None:
        """Partial update only changes specified field, preserves others."""
        created = await state_manager.create_ticket(ticket)
        original_title = ticket.title
        original_status = ticket.status

        updated = await state_manager.update_ticket(created.id, priority=new_priority)

        assert updated is not None
        assert updated.priority == new_priority
        assert updated.title == original_title
        assert updated.status == original_status


class TestTicketDelete:
    """Property-based tests for ticket deletion."""

    @integration_settings
    @given(tickets())
    async def test_delete_removes_ticket(self, state_manager, ticket: Ticket) -> None:
        """Deleting a ticket makes it unretrievable."""
        created = await state_manager.create_ticket(ticket)
        ticket_id = created.id

        result = await state_manager.delete_ticket(ticket_id)

        assert result is True
        assert await state_manager.get_ticket(ticket_id) is None

    @integration_settings
    @given(st.text(min_size=8, max_size=8, alphabet="abcdef0123456789"))
    async def test_delete_nonexistent_returns_false(self, state_manager, fake_id: str) -> None:
        """Deleting a non-existent ticket returns False."""
        result = await state_manager.delete_ticket(fake_id)
        assert result is False


class TestStatusTransitions:
    """Property-based tests for move_ticket status transitions."""

    @integration_settings
    @given(statuses, statuses)
    async def test_move_ticket_updates_status(
        self, state_manager, initial: TicketStatus, target: TicketStatus
    ) -> None:
        """move_ticket correctly changes status."""
        ticket = Ticket.create(title="Move test", status=initial)
        created = await state_manager.create_ticket(ticket)

        moved = await state_manager.move_ticket(created.id, target)

        assert moved is not None
        assert moved.status == target


# =============================================================================
# Scratchpad Properties
# =============================================================================


class TestScratchpadRoundtrip:
    """Property-based tests for scratchpad operations."""

    @integration_settings
    @given(scratchpad_content)
    async def test_scratchpad_roundtrip(self, state_manager, content: str) -> None:
        """Content within limits survives roundtrip."""
        ticket = await state_manager.create_ticket(Ticket.create(title="Scratchpad test"))

        await state_manager.update_scratchpad(ticket.id, content)
        retrieved = await state_manager.get_scratchpad(ticket.id)

        assert retrieved == content

    @slow_integration_settings
    @given(oversized_scratchpad())
    async def test_scratchpad_truncation(self, state_manager, content: str) -> None:
        """Content exceeding 50000 chars is truncated to the last 50000 chars."""
        ticket = await state_manager.create_ticket(Ticket.create(title="Truncation test"))

        await state_manager.update_scratchpad(ticket.id, content)
        retrieved = await state_manager.get_scratchpad(ticket.id)

        assert len(retrieved) == SCRATCHPAD_LIMIT
        # Should keep the LAST 50000 characters (tail truncation)
        assert retrieved == content[-SCRATCHPAD_LIMIT:]

    @integration_settings
    @given(scratchpad_content, scratchpad_content)
    async def test_scratchpad_overwrite(self, state_manager, first: str, second: str) -> None:
        """Updating scratchpad overwrites previous content."""
        ticket = await state_manager.create_ticket(Ticket.create(title="Overwrite test"))

        await state_manager.update_scratchpad(ticket.id, first)
        await state_manager.update_scratchpad(ticket.id, second)

        retrieved = await state_manager.get_scratchpad(ticket.id)
        assert retrieved == second


class TestScratchpadDelete:
    """Property-based tests for scratchpad deletion."""

    @integration_settings
    @given(scratchpad_content)
    async def test_delete_scratchpad_clears_content(self, state_manager, content: str) -> None:
        """Deleting scratchpad clears the content."""
        ticket = await state_manager.create_ticket(Ticket.create(title="Delete test"))
        await state_manager.update_scratchpad(ticket.id, content)

        await state_manager.delete_scratchpad(ticket.id)
        retrieved = await state_manager.get_scratchpad(ticket.id)

        assert retrieved == ""


# =============================================================================
# New Fields Properties
# =============================================================================


class TestNewFieldsRoundtrip:
    """Property-based tests for new ticket fields."""

    @integration_settings
    @given(st.lists(plain_text.filter(lambda x: x.strip()), min_size=0, max_size=5))
    async def test_acceptance_criteria_roundtrip(self, state_manager, criteria: list[str]) -> None:
        """Acceptance criteria list survives roundtrip."""
        ticket = Ticket.create(title="AC test", acceptance_criteria=criteria)
        created = await state_manager.create_ticket(ticket)

        retrieved = await state_manager.get_ticket(created.id)

        assert retrieved is not None
        assert retrieved.acceptance_criteria == criteria

    @integration_settings
    @given(st.booleans())
    async def test_session_active_roundtrip(self, state_manager, active: bool) -> None:
        """session_active boolean survives roundtrip."""
        ticket = Ticket.create(title="Session test", session_active=active)
        created = await state_manager.create_ticket(ticket)

        retrieved = await state_manager.get_ticket(created.id)

        assert retrieved is not None
        assert retrieved.session_active == active

    @integration_settings
    @given(st.one_of(st.none(), st.booleans()))
    async def test_checks_passed_roundtrip(self, state_manager, checks_passed: bool | None) -> None:
        """checks_passed (nullable boolean) survives roundtrip."""
        ticket = Ticket.create(title="Checks test", checks_passed=checks_passed)
        created = await state_manager.create_ticket(ticket)

        retrieved = await state_manager.get_ticket(created.id)

        assert retrieved is not None
        assert retrieved.checks_passed == checks_passed

    @integration_settings
    @given(st.one_of(st.none(), plain_text.filter(lambda x: len(x) <= 500)))
    async def test_review_summary_roundtrip(self, state_manager, summary: str | None) -> None:
        """review_summary (nullable string) survives roundtrip."""
        ticket = Ticket.create(title="Review test", review_summary=summary)
        created = await state_manager.create_ticket(ticket)

        retrieved = await state_manager.get_ticket(created.id)

        assert retrieved is not None
        assert retrieved.review_summary == summary


class TestSessionActiveToggle:
    """Property-based tests for mark_session_active."""

    @integration_settings
    @given(st.booleans(), st.booleans())
    async def test_mark_session_active_toggle(
        self, state_manager, initial: bool, target: bool
    ) -> None:
        """mark_session_active correctly toggles the flag."""
        ticket = Ticket.create(title="Toggle test", session_active=initial)
        created = await state_manager.create_ticket(ticket)

        updated = await state_manager.mark_session_active(created.id, target)

        assert updated is not None
        assert updated.session_active == target


class TestReviewSummary:
    """Property-based tests for set_review_summary."""

    @integration_settings
    @given(plain_text.filter(lambda x: len(x) <= 500), st.one_of(st.none(), st.booleans()))
    async def test_set_review_summary_roundtrip(
        self, state_manager, summary: str, checks_passed: bool | None
    ) -> None:
        """set_review_summary correctly updates both fields."""
        ticket = Ticket.create(title="Review summary test")
        created = await state_manager.create_ticket(ticket)

        updated = await state_manager.set_review_summary(created.id, summary, checks_passed)

        assert updated is not None
        assert updated.review_summary == summary
        assert updated.checks_passed == checks_passed


# =============================================================================
# Ticket Counts and Filtering
# =============================================================================


class TestTicketCounts:
    """Property-based tests for get_ticket_counts."""

    @slow_integration_settings
    @given(st.lists(statuses, min_size=0, max_size=10))
    async def test_ticket_counts_increase_correctly(
        self, state_manager, status_list: list[TicketStatus]
    ) -> None:
        """Counts increase correctly when tickets are added."""
        # Get initial counts
        initial_counts = await state_manager.get_ticket_counts()

        # Create tickets with the given statuses
        for i, status in enumerate(status_list):
            ticket = Ticket.create(title=f"Count test {i}", status=status)
            await state_manager.create_ticket(ticket)

        final_counts = await state_manager.get_ticket_counts()

        # Calculate expected increments
        expected_increments = {status: 0 for status in TicketStatus}
        for status in status_list:
            expected_increments[status] += 1

        for status in TicketStatus:
            expected = initial_counts[status] + expected_increments[status]
            assert final_counts[status] == expected, f"Mismatch for {status}"


class TestTicketsByStatus:
    """Property-based tests for get_tickets_by_status."""

    @integration_settings
    @given(statuses, st.integers(min_value=1, max_value=5))
    async def test_get_tickets_by_status_returns_created_tickets(
        self, state_manager, status: TicketStatus, count: int
    ) -> None:
        """get_tickets_by_status returns the tickets we just created with that status."""
        # Create 'count' tickets with the target status using unique identifiers
        import uuid

        batch_id = uuid.uuid4().hex[:8]
        created_ids = []
        for i in range(count):
            ticket = Ticket.create(
                title=f"Batch {batch_id} Status {status.value} #{i}", status=status
            )
            created = await state_manager.create_ticket(ticket)
            created_ids.append(created.id)

        tickets_by_status = await state_manager.get_tickets_by_status(status)

        # All created tickets should be in the result
        result_ids = {t.id for t in tickets_by_status}
        for created_id in created_ids:
            assert created_id in result_ids, f"Created ticket {created_id} not found in results"

        # All returned tickets should have the correct status
        for t in tickets_by_status:
            assert t.status == status
