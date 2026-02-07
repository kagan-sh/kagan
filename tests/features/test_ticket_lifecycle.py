"""Feature tests for Ticket Lifecycle.

Tests organized by user-facing features, not implementation layers.
Each test validates a complete user journey or critical behavior.

Covers:
- Ticket CRUD (create, read, update, delete)
- Status transitions (BACKLOG -> IN_PROGRESS -> REVIEW -> DONE)
- Ticket types (AUTO, PAIR)
- Merge workflow
- Review workflow (approve/reject)
- Rejection handling
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from tests.helpers.mocks import create_mock_session_manager, create_test_config

from kagan.agents.scheduler import Scheduler
from kagan.agents.worktree import WorktreeManager
from kagan.database.models import (
    MergeReadiness,
    Ticket,
    TicketPriority,
    TicketStatus,
    TicketType,
)
from kagan.lifecycle.ticket_lifecycle import TicketLifecycle

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.database.manager import StateManager


# =============================================================================
# Feature: Create Ticket
# =============================================================================


class TestCreateTicket:
    """User can create tickets of different types."""

    async def test_create_pair_ticket_in_backlog(self, state_manager: StateManager):
        """Creating PAIR ticket adds it to BACKLOG with default values."""
        ticket = Ticket.create(
            title="Implement login feature",
            description="Add OAuth login support",
            priority=TicketPriority.HIGH,
            ticket_type=TicketType.PAIR,
        )

        created = await state_manager.create_ticket(ticket)

        assert created.id == ticket.id
        assert created.status == TicketStatus.BACKLOG
        assert created.ticket_type == TicketType.PAIR
        assert created.title == "Implement login feature"

        # Verify persisted
        fetched = await state_manager.get_ticket(ticket.id)
        assert fetched is not None
        assert fetched.title == "Implement login feature"

    async def test_create_auto_ticket_in_backlog(self, state_manager: StateManager):
        """Creating AUTO ticket adds it to BACKLOG ready for agent execution."""
        ticket = Ticket.create(
            title="Refactor database layer",
            description="Move to async queries",
            priority=TicketPriority.MEDIUM,
            ticket_type=TicketType.AUTO,
        )

        created = await state_manager.create_ticket(ticket)

        assert created.status == TicketStatus.BACKLOG
        assert created.ticket_type == TicketType.AUTO
        assert created.total_iterations == 0

    async def test_create_ticket_with_acceptance_criteria(self, state_manager: StateManager):
        """Tickets can have acceptance criteria for validation."""
        criteria = ["User can log in", "Session persists across refresh", "Logout clears session"]
        ticket = Ticket.create(
            title="Add session management",
            acceptance_criteria=criteria,
        )

        await state_manager.create_ticket(ticket)
        fetched = await state_manager.get_ticket(ticket.id)

        assert fetched is not None
        assert fetched.acceptance_criteria == criteria


# =============================================================================
# Feature: Read/View Ticket
# =============================================================================


class TestReadTicket:
    """User can view ticket details and list tickets."""

    async def test_get_ticket_by_id(self, state_manager: StateManager):
        """Fetching ticket by ID returns complete data."""
        ticket = Ticket.create(
            title="Test ticket",
            description="Full description with details",
            priority=TicketPriority.LOW,
        )
        await state_manager.create_ticket(ticket)

        fetched = await state_manager.get_ticket(ticket.id)

        assert fetched is not None
        assert fetched.title == "Test ticket"
        assert fetched.description == "Full description with details"
        assert fetched.priority == TicketPriority.LOW

    async def test_get_nonexistent_ticket_returns_none(self, state_manager: StateManager):
        """Fetching nonexistent ticket returns None."""
        result = await state_manager.get_ticket("nonexistent-id")
        assert result is None

    async def test_list_tickets_by_status(self, state_manager: StateManager):
        """Tickets can be filtered by status."""
        backlog = Ticket.create(title="Backlog 1", status=TicketStatus.BACKLOG)
        in_progress = Ticket.create(title="WIP 1", status=TicketStatus.IN_PROGRESS)
        await state_manager.create_ticket(backlog)
        await state_manager.create_ticket(in_progress)

        backlog_tickets = await state_manager.get_tickets_by_status(TicketStatus.BACKLOG)
        wip_tickets = await state_manager.get_tickets_by_status(TicketStatus.IN_PROGRESS)

        assert len(backlog_tickets) == 1
        assert backlog_tickets[0].title == "Backlog 1"
        assert len(wip_tickets) == 1
        assert wip_tickets[0].title == "WIP 1"


# =============================================================================
# Feature: Update Ticket
# =============================================================================


class TestUpdateTicket:
    """User can modify ticket properties."""

    async def test_update_ticket_title(self, state_manager: StateManager):
        """Updating title persists the change."""
        ticket = Ticket.create(title="Original title")
        await state_manager.create_ticket(ticket)

        updated = await state_manager.update_ticket(ticket.id, title="Updated title")

        assert updated is not None
        assert updated.title == "Updated title"

    async def test_update_ticket_priority(self, state_manager: StateManager):
        """Updating priority changes ordering behavior."""
        ticket = Ticket.create(title="Test", priority=TicketPriority.LOW)
        await state_manager.create_ticket(ticket)

        updated = await state_manager.update_ticket(ticket.id, priority=TicketPriority.HIGH)

        assert updated is not None
        assert updated.priority == TicketPriority.HIGH

    async def test_update_multiple_fields(self, state_manager: StateManager):
        """Multiple fields can be updated atomically."""
        ticket = Ticket.create(
            title="Original",
            description="Old desc",
            priority=TicketPriority.LOW,
        )
        await state_manager.create_ticket(ticket)

        updated = await state_manager.update_ticket(
            ticket.id,
            title="New title",
            description="New description",
            priority=TicketPriority.HIGH,
        )

        assert updated is not None
        assert updated.title == "New title"
        assert updated.description == "New description"
        assert updated.priority == TicketPriority.HIGH


# =============================================================================
# Feature: Delete Ticket
# =============================================================================


class TestDeleteTicket:
    """User can remove tickets with proper cleanup."""

    async def test_delete_ticket_removes_from_database(self, state_manager: StateManager):
        """Deleting ticket removes it from database."""
        ticket = Ticket.create(title="To be deleted")
        await state_manager.create_ticket(ticket)

        result = await state_manager.delete_ticket(ticket.id)

        assert result is True
        assert await state_manager.get_ticket(ticket.id) is None

    async def test_delete_nonexistent_ticket_returns_false(self, state_manager: StateManager):
        """Deleting nonexistent ticket returns False."""
        result = await state_manager.delete_ticket("nonexistent-id")
        assert result is False

    async def test_delete_with_lifecycle_cleanup(self, state_manager: StateManager, git_repo: Path):
        """TicketLifecycle.delete_ticket cleans up worktree and session."""
        ticket = Ticket.create(
            title="Task with resources",
            status=TicketStatus.IN_PROGRESS,
            ticket_type=TicketType.PAIR,
        )
        await state_manager.create_ticket(ticket)

        config = create_test_config()
        worktrees = WorktreeManager(git_repo)
        sessions = create_mock_session_manager()
        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=worktrees,
            config=config,
        )

        # Create worktree to be cleaned up
        await worktrees.create(ticket.id, ticket.title)
        assert await worktrees.get_path(ticket.id) is not None

        lifecycle = TicketLifecycle(
            state=state_manager,
            worktrees=worktrees,
            sessions=sessions,
            scheduler=scheduler,
            config=config,
        )

        success, _message = await lifecycle.delete_ticket(ticket)

        assert success
        assert await state_manager.get_ticket(ticket.id) is None
        assert await worktrees.get_path(ticket.id) is None
        sessions.kill_session.assert_called_once_with(ticket.id)


# =============================================================================
# Feature: Status Transitions
# =============================================================================


class TestStatusTransitions:
    """Tickets move through status workflow correctly."""

    async def test_backlog_to_in_progress(self, state_manager: StateManager):
        """Moving from BACKLOG to IN_PROGRESS is valid."""
        ticket = Ticket.create(title="Start work", status=TicketStatus.BACKLOG)
        await state_manager.create_ticket(ticket)

        updated = await state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)

        assert updated is not None
        assert updated.status == TicketStatus.IN_PROGRESS

    async def test_in_progress_to_review(self, state_manager: StateManager):
        """Moving from IN_PROGRESS to REVIEW is valid."""
        ticket = Ticket.create(title="Ready for review", status=TicketStatus.IN_PROGRESS)
        await state_manager.create_ticket(ticket)

        updated = await state_manager.move_ticket(ticket.id, TicketStatus.REVIEW)

        assert updated is not None
        assert updated.status == TicketStatus.REVIEW

    async def test_review_to_done(self, state_manager: StateManager):
        """Moving from REVIEW to DONE is valid (typically via merge)."""
        ticket = Ticket.create(title="Approved", status=TicketStatus.REVIEW)
        await state_manager.create_ticket(ticket)

        updated = await state_manager.move_ticket(ticket.id, TicketStatus.DONE)

        assert updated is not None
        assert updated.status == TicketStatus.DONE

    async def test_move_backward_review_to_in_progress(self, state_manager: StateManager):
        """Moving backward from REVIEW to IN_PROGRESS (rejection) is valid."""
        ticket = Ticket.create(title="Needs work", status=TicketStatus.REVIEW)
        await state_manager.create_ticket(ticket)

        updated = await state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)

        assert updated is not None
        assert updated.status == TicketStatus.IN_PROGRESS

    async def test_next_status_helper(self):
        """TicketStatus.next_status returns correct next status."""
        assert TicketStatus.next_status(TicketStatus.BACKLOG) == TicketStatus.IN_PROGRESS
        assert TicketStatus.next_status(TicketStatus.IN_PROGRESS) == TicketStatus.REVIEW
        assert TicketStatus.next_status(TicketStatus.REVIEW) == TicketStatus.DONE
        assert TicketStatus.next_status(TicketStatus.DONE) is None

    async def test_prev_status_helper(self):
        """TicketStatus.prev_status returns correct previous status."""
        assert TicketStatus.prev_status(TicketStatus.DONE) == TicketStatus.REVIEW
        assert TicketStatus.prev_status(TicketStatus.REVIEW) == TicketStatus.IN_PROGRESS
        assert TicketStatus.prev_status(TicketStatus.IN_PROGRESS) == TicketStatus.BACKLOG
        assert TicketStatus.prev_status(TicketStatus.BACKLOG) is None

    async def test_status_change_callback_triggered(self, state_manager: StateManager):
        """Status change triggers callback for reactive systems."""
        ticket = Ticket.create(title="Callback test", status=TicketStatus.BACKLOG)

        changes: list[tuple[str, TicketStatus | None, TicketStatus | None]] = []

        def on_change(tid: str, old: TicketStatus | None, new: TicketStatus | None) -> None:
            changes.append((tid, old, new))

        state_manager.set_status_change_callback(on_change)

        await state_manager.create_ticket(ticket)
        await state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)

        assert len(changes) == 2
        assert changes[0] == (ticket.id, None, TicketStatus.BACKLOG)  # create
        assert changes[1] == (ticket.id, TicketStatus.BACKLOG, TicketStatus.IN_PROGRESS)  # move


# =============================================================================
# Feature: Ticket Types (AUTO vs PAIR)
# =============================================================================


class TestTicketTypes:
    """Different ticket types have different behaviors."""

    async def test_auto_ticket_has_iteration_tracking(self, state_manager: StateManager):
        """AUTO tickets track iteration count for agent execution."""
        ticket = Ticket.create(
            title="Auto task",
            ticket_type=TicketType.AUTO,
        )
        await state_manager.create_ticket(ticket)

        # Increment iterations
        await state_manager.increment_total_iterations(ticket.id)
        await state_manager.increment_total_iterations(ticket.id)

        fetched = await state_manager.get_ticket(ticket.id)
        assert fetched is not None
        assert fetched.total_iterations == 2

    async def test_pair_ticket_tracks_session_active(self, state_manager: StateManager):
        """PAIR tickets track whether tmux session is active."""
        ticket = Ticket.create(
            title="Pair task",
            ticket_type=TicketType.PAIR,
        )
        await state_manager.create_ticket(ticket)

        updated = await state_manager.mark_session_active(ticket.id, True)

        assert updated is not None
        assert updated.session_active is True

        updated2 = await state_manager.mark_session_active(ticket.id, False)
        assert updated2 is not None
        assert updated2.session_active is False


# =============================================================================
# Feature: Merge Workflow
# =============================================================================


class TestMergeWorkflow:
    """Approved tickets can be merged to main branch."""

    async def test_merge_updates_ticket_state_on_success(
        self, state_manager: StateManager, git_repo: Path
    ):
        """Successful merge moves ticket to DONE with proper state."""
        ticket = Ticket.create(
            title="Ready to merge",
            status=TicketStatus.REVIEW,
            ticket_type=TicketType.PAIR,
            checks_passed=True,
        )
        await state_manager.create_ticket(ticket)

        config = create_test_config()
        worktrees = WorktreeManager(git_repo)
        sessions = create_mock_session_manager()
        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=worktrees,
            config=config,
        )

        # Mock the merge to succeed (actual git merge tested in git_workflows tests)
        from unittest.mock import AsyncMock

        worktrees.merge_to_main = AsyncMock(return_value=(True, "Merged successfully"))
        worktrees.delete = AsyncMock()

        lifecycle = TicketLifecycle(
            state=state_manager,
            worktrees=worktrees,
            sessions=sessions,
            scheduler=scheduler,
            config=config,
        )

        success, message = await lifecycle.merge_ticket(ticket)

        assert success, f"Merge failed: {message}"
        fetched = await state_manager.get_ticket(ticket.id)
        assert fetched is not None
        assert fetched.status == TicketStatus.DONE
        assert fetched.merge_readiness == MergeReadiness.READY
        assert fetched.merge_failed is False

        # Verify cleanup was called
        worktrees.delete.assert_called_once()
        sessions.kill_session.assert_called_once_with(ticket.id)

    async def test_merge_conflict_sets_blocked_state(
        self, state_manager: StateManager, git_repo: Path
    ):
        """Merge conflict marks ticket as blocked with error details."""
        ticket = Ticket.create(
            title="Has conflicts",
            status=TicketStatus.REVIEW,
            ticket_type=TicketType.PAIR,
            checks_passed=True,
        )
        await state_manager.create_ticket(ticket)

        config = create_test_config()
        worktrees = WorktreeManager(git_repo)
        sessions = create_mock_session_manager()
        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=worktrees,
            config=config,
        )

        # Mock the merge to fail with conflict
        from unittest.mock import AsyncMock

        worktrees.merge_to_main = AsyncMock(
            return_value=(False, "CONFLICT (content): Merge conflict in file.py")
        )

        lifecycle = TicketLifecycle(
            state=state_manager,
            worktrees=worktrees,
            sessions=sessions,
            scheduler=scheduler,
            config=config,
        )

        success, _message = await lifecycle.merge_ticket(ticket)

        assert success is False
        fetched = await state_manager.get_ticket(ticket.id)
        assert fetched is not None
        assert fetched.status == TicketStatus.REVIEW  # Stays in REVIEW
        assert fetched.merge_failed is True
        assert fetched.merge_readiness == MergeReadiness.BLOCKED
        assert fetched.merge_error is not None
        assert "conflict" in fetched.merge_error.lower()

    async def test_merge_blocked_without_approval(
        self, state_manager: StateManager, git_repo: Path
    ):
        """Merge blocked when require_review_approval is True and not approved."""
        ticket = Ticket.create(
            title="Not approved yet",
            status=TicketStatus.REVIEW,
            checks_passed=None,  # Not yet approved
        )
        await state_manager.create_ticket(ticket)

        from kagan.config import AgentConfig, GeneralConfig, KaganConfig

        config = KaganConfig(
            general=GeneralConfig(
                require_review_approval=True,
                default_worker_agent="test",
            ),
            agents={
                "test": AgentConfig(
                    identity="test",
                    name="Test",
                    short_name="test",
                    run_command={"*": "echo"},
                )
            },
        )

        worktrees = WorktreeManager(git_repo)
        sessions = create_mock_session_manager()
        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=worktrees,
            config=config,
        )

        lifecycle = TicketLifecycle(
            state=state_manager,
            worktrees=worktrees,
            sessions=sessions,
            scheduler=scheduler,
            config=config,
        )

        success, message = await lifecycle.merge_ticket(ticket)

        assert success is False
        assert "approval required" in message.lower()

        fetched = await state_manager.get_ticket(ticket.id)
        assert fetched is not None
        assert fetched.merge_failed is True
        assert fetched.merge_readiness == MergeReadiness.BLOCKED


# =============================================================================
# Feature: Review Workflow
# =============================================================================


class TestReviewWorkflow:
    """Review process with approve/reject decisions."""

    async def test_set_review_summary_and_checks(self, state_manager: StateManager):
        """Review results update ticket with summary and check status."""
        ticket = Ticket.create(
            title="Under review",
            status=TicketStatus.REVIEW,
        )
        await state_manager.create_ticket(ticket)

        updated = await state_manager.set_review_summary(
            ticket.id,
            summary="LGTM - implementation follows best practices",
            checks_passed=True,
        )

        assert updated is not None
        assert updated.review_summary == "LGTM - implementation follows best practices"
        assert updated.checks_passed is True

    async def test_review_rejection_updates_checks(self, state_manager: StateManager):
        """Failed review updates checks_passed to False."""
        ticket = Ticket.create(
            title="Needs fixes",
            status=TicketStatus.REVIEW,
        )
        await state_manager.create_ticket(ticket)

        updated = await state_manager.set_review_summary(
            ticket.id,
            summary="Missing error handling in login flow",
            checks_passed=False,
        )

        assert updated is not None
        assert updated.checks_passed is False


# =============================================================================
# Feature: Rejection Handling
# =============================================================================


class TestRejectionHandling:
    """Handle rejected reviews with retry/stage/shelve actions."""

    async def test_rejection_retry_moves_to_in_progress(
        self, state_manager: StateManager, git_repo: Path
    ):
        """Retry action moves ticket to IN_PROGRESS with feedback."""
        ticket = Ticket.create(
            title="Retry needed",
            description="Original task",
            status=TicketStatus.REVIEW,
            ticket_type=TicketType.AUTO,
        )
        await state_manager.create_ticket(ticket)

        config = create_test_config()
        worktrees = WorktreeManager(git_repo)
        sessions = create_mock_session_manager()
        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=worktrees,
            config=config,
        )

        lifecycle = TicketLifecycle(
            state=state_manager,
            worktrees=worktrees,
            sessions=sessions,
            scheduler=scheduler,
            config=config,
        )

        updated = await lifecycle.apply_rejection_feedback(
            ticket,
            feedback="Add input validation",
            action="retry",
        )

        assert updated.status == TicketStatus.IN_PROGRESS
        assert "Add input validation" in updated.description
        assert "Review Feedback" in updated.description
        assert updated.merge_failed is False
        assert updated.merge_readiness == MergeReadiness.RISK

    async def test_rejection_shelve_moves_to_backlog(
        self, state_manager: StateManager, git_repo: Path
    ):
        """Shelve action moves ticket to BACKLOG preserving iterations."""
        ticket = Ticket.create(
            title="Shelve for later",
            status=TicketStatus.REVIEW,
            ticket_type=TicketType.AUTO,
        )
        await state_manager.create_ticket(ticket)

        config = create_test_config()
        worktrees = WorktreeManager(git_repo)
        sessions = create_mock_session_manager()
        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=worktrees,
            config=config,
        )

        lifecycle = TicketLifecycle(
            state=state_manager,
            worktrees=worktrees,
            sessions=sessions,
            scheduler=scheduler,
            config=config,
        )

        updated = await lifecycle.apply_rejection_feedback(
            ticket,
            feedback="Not a priority right now",
            action="shelve",
        )

        assert updated.status == TicketStatus.BACKLOG
        assert "Not a priority right now" in updated.description

    async def test_rejection_stage_moves_to_in_progress(
        self, state_manager: StateManager, git_repo: Path
    ):
        """Stage action moves to IN_PROGRESS for manual restart."""
        ticket = Ticket.create(
            title="Stage for manual work",
            status=TicketStatus.REVIEW,
            ticket_type=TicketType.AUTO,
        )
        await state_manager.create_ticket(ticket)

        config = create_test_config()
        worktrees = WorktreeManager(git_repo)
        sessions = create_mock_session_manager()
        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=worktrees,
            config=config,
        )

        lifecycle = TicketLifecycle(
            state=state_manager,
            worktrees=worktrees,
            sessions=sessions,
            scheduler=scheduler,
            config=config,
        )

        updated = await lifecycle.apply_rejection_feedback(
            ticket,
            feedback="Needs manual intervention",
            action="stage",
        )

        assert updated.status == TicketStatus.IN_PROGRESS

    async def test_rejection_without_feedback(self, state_manager: StateManager, git_repo: Path):
        """Rejection works without feedback text."""
        ticket = Ticket.create(
            title="No feedback rejection",
            status=TicketStatus.REVIEW,
            ticket_type=TicketType.AUTO,
        )
        await state_manager.create_ticket(ticket)

        config = create_test_config()
        worktrees = WorktreeManager(git_repo)
        sessions = create_mock_session_manager()
        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=worktrees,
            config=config,
        )

        lifecycle = TicketLifecycle(
            state=state_manager,
            worktrees=worktrees,
            sessions=sessions,
            scheduler=scheduler,
            config=config,
        )

        updated = await lifecycle.apply_rejection_feedback(
            ticket,
            feedback=None,
            action="shelve",
        )

        assert updated.status == TicketStatus.BACKLOG
        # Original description unchanged when no feedback
        events = await state_manager.get_ticket_events(ticket.id)
        assert any("Rejected" in e.message for e in events)


# =============================================================================
# Feature: Exploratory Close
# =============================================================================


class TestExploratoryClose:
    """Tickets without changes can be closed without merge."""

    async def test_close_exploratory_deletes_ticket(
        self, state_manager: StateManager, git_repo: Path
    ):
        """Closing exploratory ticket deletes it (no merge needed)."""
        ticket = Ticket.create(
            title="Exploratory investigation",
            status=TicketStatus.IN_PROGRESS,
            ticket_type=TicketType.PAIR,
        )
        await state_manager.create_ticket(ticket)

        config = create_test_config()
        worktrees = WorktreeManager(git_repo)
        sessions = create_mock_session_manager()
        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=worktrees,
            config=config,
        )

        await worktrees.create(ticket.id, ticket.title)

        lifecycle = TicketLifecycle(
            state=state_manager,
            worktrees=worktrees,
            sessions=sessions,
            scheduler=scheduler,
            config=config,
        )

        success, message = await lifecycle.close_exploratory(ticket)

        assert success
        assert "exploratory" in message.lower()
        assert await state_manager.get_ticket(ticket.id) is None
        assert await worktrees.get_path(ticket.id) is None


# =============================================================================
# Feature: Scratchpad Persistence
# =============================================================================


class TestScratchpadPersistence:
    """Agent working notes persist across operations."""

    async def test_scratchpad_create_and_read(self, state_manager: StateManager):
        """Scratchpad can be created and read back."""
        ticket = Ticket.create(title="With scratchpad")
        await state_manager.create_ticket(ticket)

        await state_manager.update_scratchpad(ticket.id, "Agent notes: investigating issue...")

        content = await state_manager.get_scratchpad(ticket.id)
        assert content == "Agent notes: investigating issue..."

    async def test_scratchpad_update_replaces(self, state_manager: StateManager):
        """Updating scratchpad replaces previous content."""
        ticket = Ticket.create(title="Scratchpad update")
        await state_manager.create_ticket(ticket)

        await state_manager.update_scratchpad(ticket.id, "First notes")
        await state_manager.update_scratchpad(ticket.id, "Updated notes")

        content = await state_manager.get_scratchpad(ticket.id)
        assert content == "Updated notes"

    async def test_scratchpad_empty_by_default(self, state_manager: StateManager):
        """New tickets have empty scratchpad."""
        ticket = Ticket.create(title="No scratchpad")
        await state_manager.create_ticket(ticket)

        content = await state_manager.get_scratchpad(ticket.id)
        assert content == ""


# =============================================================================
# Feature: Ticket Events (Audit Trail)
# =============================================================================


class TestTicketEvents:
    """Ticket actions create audit trail events."""

    async def test_append_and_get_events(self, state_manager: StateManager):
        """Events can be appended and retrieved."""
        ticket = Ticket.create(title="Event tracking")
        await state_manager.create_ticket(ticket)

        await state_manager.append_ticket_event(ticket.id, "merge", "Merged to main")
        await state_manager.append_ticket_event(ticket.id, "review", "Review passed")

        events = await state_manager.get_ticket_events(ticket.id)

        assert len(events) == 2
        # Most recent first
        assert events[0].event_type == "review"
        assert events[1].event_type == "merge"

    async def test_events_limited_by_default(self, state_manager: StateManager):
        """get_ticket_events respects limit parameter."""
        ticket = Ticket.create(title="Many events")
        await state_manager.create_ticket(ticket)

        for i in range(25):
            await state_manager.append_ticket_event(ticket.id, "test", f"Event {i}")

        events = await state_manager.get_ticket_events(ticket.id, limit=10)
        assert len(events) == 10


# =============================================================================
# Feature: Search
# =============================================================================


class TestTicketSearch:
    """Users can search tickets by title/description."""

    async def test_search_by_title(self, state_manager: StateManager):
        """Search finds tickets by title match."""
        ticket1 = Ticket.create(title="Fix authentication bug")
        ticket2 = Ticket.create(title="Add logging feature")
        await state_manager.create_ticket(ticket1)
        await state_manager.create_ticket(ticket2)

        results = await state_manager.search_tickets("authentication")

        assert len(results) == 1
        assert results[0].title == "Fix authentication bug"

    async def test_search_by_description(self, state_manager: StateManager):
        """Search finds tickets by description match."""
        ticket = Ticket.create(
            title="Feature X",
            description="Implement OAuth2 flow for external services",
        )
        await state_manager.create_ticket(ticket)

        results = await state_manager.search_tickets("OAuth2")

        assert len(results) == 1
        assert results[0].id == ticket.id

    async def test_search_empty_query_returns_empty(self, state_manager: StateManager):
        """Empty search query returns no results."""
        ticket = Ticket.create(title="Should not appear")
        await state_manager.create_ticket(ticket)

        results = await state_manager.search_tickets("")
        assert len(results) == 0


# =============================================================================
# Unit Tests: Merge Conflict Parsing
# =============================================================================


@pytest.mark.unit
class TestParseConflictFiles:
    """Unit tests for _parse_conflict_files() helper function."""

    def test_parse_single_conflict(self):
        """Parses single file conflict from git output."""
        from kagan.lifecycle.ticket_lifecycle import _parse_conflict_files

        git_output = "CONFLICT (content): Merge conflict in src/app.py"
        result = _parse_conflict_files(git_output)

        assert result == ["src/app.py"]

    def test_parse_multiple_conflicts(self):
        """Parses multiple file conflicts from git output."""
        from kagan.lifecycle.ticket_lifecycle import _parse_conflict_files

        git_output = """
CONFLICT (content): Merge conflict in src/database/models.py
CONFLICT (content): Merge conflict in tests/test_app.py
CONFLICT (content): Merge conflict in README.md
        """
        result = _parse_conflict_files(git_output)

        assert len(result) == 3
        assert "src/database/models.py" in result
        assert "tests/test_app.py" in result
        assert "README.md" in result

    def test_parse_rename_conflict(self):
        """Parses rename/delete conflicts from git output."""
        from kagan.lifecycle.ticket_lifecycle import _parse_conflict_files

        git_output = "CONFLICT (rename/delete): Merge conflict in old_file.py"
        result = _parse_conflict_files(git_output)

        assert result == ["old_file.py"]

    def test_parse_modify_delete_conflict(self):
        """Parses modify/delete conflicts from git output."""
        from kagan.lifecycle.ticket_lifecycle import _parse_conflict_files

        git_output = "CONFLICT (modify/delete): Merge conflict in config.yaml"
        result = _parse_conflict_files(git_output)

        assert result == ["config.yaml"]

    def test_parse_empty_output(self):
        """Returns empty list for empty output."""
        from kagan.lifecycle.ticket_lifecycle import _parse_conflict_files

        result = _parse_conflict_files("")

        assert result == []

    def test_parse_no_conflicts(self):
        """Returns empty list when no conflicts in output."""
        from kagan.lifecycle.ticket_lifecycle import _parse_conflict_files

        git_output = """
Merging:
Auto-merging src/app.py
Fast-forward merge completed successfully
        """
        result = _parse_conflict_files(git_output)

        assert result == []

    def test_parse_multiline_with_other_text(self):
        """Extracts conflicts from verbose git output."""
        from kagan.lifecycle.ticket_lifecycle import _parse_conflict_files

        git_output = """
Auto-merging src/utils.py
CONFLICT (content): Merge conflict in src/utils.py
Automatic merge failed; fix conflicts and then commit the result.
Auto-merging tests/test_utils.py
CONFLICT (content): Merge conflict in tests/test_utils.py
        """
        result = _parse_conflict_files(git_output)

        assert len(result) == 2
        assert "src/utils.py" in result
        assert "tests/test_utils.py" in result

    def test_parse_file_with_spaces(self):
        """Handles file paths with spaces correctly."""
        from kagan.lifecycle.ticket_lifecycle import _parse_conflict_files

        git_output = "CONFLICT (content): Merge conflict in my documents/file name.txt"
        result = _parse_conflict_files(git_output)

        assert result == ["my documents/file name.txt"]

    def test_parse_file_with_special_chars(self):
        """Handles file paths with special characters."""
        from kagan.lifecycle.ticket_lifecycle import _parse_conflict_files

        git_output = "CONFLICT (content): Merge conflict in src/user-config_v2.0.py"
        result = _parse_conflict_files(git_output)

        assert result == ["src/user-config_v2.0.py"]


@pytest.mark.unit
class TestIsMergeConflict:
    """Unit tests for _is_merge_conflict() helper function."""

    def test_detects_conflict_keyword(self):
        """Detects 'CONFLICT' keyword in message."""
        from kagan.lifecycle.ticket_lifecycle import _is_merge_conflict

        assert _is_merge_conflict("CONFLICT (content): Merge conflict in app.py")

    def test_detects_merge_conflict_phrase(self):
        """Detects 'Merge conflict' phrase in message."""
        from kagan.lifecycle.ticket_lifecycle import _is_merge_conflict

        assert _is_merge_conflict("Merge conflict in src/database.py")

    def test_detects_conflict_in_phrase(self):
        """Detects 'conflict in' phrase in message."""
        from kagan.lifecycle.ticket_lifecycle import _is_merge_conflict

        assert _is_merge_conflict("There is a conflict in the file")

    def test_detects_fix_conflicts_phrase(self):
        """Detects 'fix conflicts' phrase in message."""
        from kagan.lifecycle.ticket_lifecycle import _is_merge_conflict

        assert _is_merge_conflict("Please fix conflicts and commit")

    def test_case_insensitive_detection(self):
        """Conflict detection is case-insensitive."""
        from kagan.lifecycle.ticket_lifecycle import _is_merge_conflict

        assert _is_merge_conflict("conflict detected")
        assert _is_merge_conflict("CONFLICT DETECTED")
        assert _is_merge_conflict("Conflict Detected")
        assert _is_merge_conflict("merge CONFLICT in file")

    def test_no_conflict_in_success_message(self):
        """Returns False for successful merge messages."""
        from kagan.lifecycle.ticket_lifecycle import _is_merge_conflict

        assert not _is_merge_conflict("Merge completed successfully")
        assert not _is_merge_conflict("Fast-forward merge")
        assert not _is_merge_conflict("Already up to date")

    def test_no_conflict_in_empty_message(self):
        """Returns False for empty message."""
        from kagan.lifecycle.ticket_lifecycle import _is_merge_conflict

        assert not _is_merge_conflict("")

    def test_no_conflict_in_unrelated_error(self):
        """Returns False for non-conflict errors."""
        from kagan.lifecycle.ticket_lifecycle import _is_merge_conflict

        assert not _is_merge_conflict("Permission denied")
        assert not _is_merge_conflict("Network error occurred")
        assert not _is_merge_conflict("Branch not found")

    def test_conflict_in_multiline_message(self):
        """Detects conflicts in multi-line messages."""
        from kagan.lifecycle.ticket_lifecycle import _is_merge_conflict

        message = """
Auto-merging src/app.py
CONFLICT (content): Merge conflict in src/app.py
Automatic merge failed; fix conflicts and then commit the result.
        """
        assert _is_merge_conflict(message)

    def test_partial_word_match_excluded(self):
        """Does not match partial words like 'conflicting' unless 'conflict' is found."""
        from kagan.lifecycle.ticket_lifecycle import _is_merge_conflict

        # These should match because they contain "conflict"
        assert _is_merge_conflict("conflicting changes detected")
        assert _is_merge_conflict("conflicts found in files")

    def test_no_false_positives(self):
        """Does not detect conflicts in messages about conflict resolution."""
        from kagan.lifecycle.ticket_lifecycle import _is_merge_conflict

        # Should NOT match - these don't contain the conflict indicators
        assert not _is_merge_conflict("All issues resolved")
        assert not _is_merge_conflict("Merge strategy applied")
        assert not _is_merge_conflict("Branch merged without issues")
