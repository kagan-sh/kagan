"""Tests for auto-merge functionality with agent-based review."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.agents.scheduler import Scheduler
from kagan.database.models import TicketCreate, TicketStatus, TicketType

if TYPE_CHECKING:
    from kagan.database.manager import StateManager

pytestmark = pytest.mark.integration


@pytest.fixture
def auto_merge_scheduler(
    state_manager, mock_worktree_manager, auto_merge_config, mock_session_manager, mocker
):
    """Create a scheduler with auto_merge enabled."""

    # Add mock methods for review prompt building
    mock_worktree_manager.get_commit_log = mocker.AsyncMock(return_value=["feat: add feature"])
    mock_worktree_manager.get_diff_stats = mocker.AsyncMock(return_value="1 file changed")
    changed_callback = mocker.MagicMock()
    return Scheduler(
        state_manager=state_manager,
        worktree_manager=mock_worktree_manager,
        config=auto_merge_config,
        session_manager=mock_session_manager,
        on_ticket_changed=changed_callback,
    )


class TestAutoMerge:
    """Tests for auto-merge functionality with agent-based review."""

    async def test_auto_merge_when_review_approved(
        self,
        auto_merge_scheduler,
        state_manager: StateManager,
        mock_worktree_manager,
        mock_session_manager,
        mock_review_agent,
        mocker,
    ):
        """Test auto-merge happens when auto_merge=true and review is approved."""
        ticket = await state_manager.create_ticket(
            TicketCreate(
                title="Auto ticket",
                ticket_type=TicketType.AUTO,
                status=TicketStatus.IN_PROGRESS,
            )
        )

        # Mock merge success
        mock_worktree_manager.merge_to_main = mocker.AsyncMock(return_value=(True, "Merged"))
        mock_worktree_manager.delete = mocker.AsyncMock()

        # Mock review agent returning approve signal
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_review_agent)

        full_ticket = await state_manager.get_ticket(ticket.id)
        assert full_ticket is not None
        await auto_merge_scheduler._handle_complete(full_ticket)

        # Ticket should be in DONE
        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.DONE
        assert updated.checks_passed is True
        assert updated.review_summary == "Implementation complete"

        # Merge and cleanup should have been called
        mock_worktree_manager.merge_to_main.assert_called_once()
        mock_worktree_manager.delete.assert_called_once()
        mock_session_manager.kill_session.assert_called_once_with(ticket.id)

    async def test_no_auto_merge_when_disabled(
        self,
        scheduler,  # Uses default config (auto_merge=false)
        state_manager: StateManager,
        mock_worktree_manager,
        mocker,
    ):
        """Test no auto-merge when auto_merge=false."""
        ticket = await state_manager.create_ticket(
            TicketCreate(
                title="Auto ticket",
                ticket_type=TicketType.AUTO,
                status=TicketStatus.IN_PROGRESS,
            )
        )

        # Add mock methods for review
        mock_worktree_manager.get_commit_log = mocker.AsyncMock(return_value=["feat: add feature"])
        mock_worktree_manager.get_diff_stats = mocker.AsyncMock(return_value="1 file changed")

        # Mock review agent returning approve signal
        mock_agent = mocker.MagicMock()
        mock_agent.set_auto_approve = mocker.MagicMock()
        mock_agent.start = mocker.MagicMock()
        mock_agent.wait_ready = mocker.AsyncMock()
        mock_agent.send_prompt = mocker.AsyncMock()
        mock_agent.get_response_text = mocker.MagicMock(return_value='<approve summary="Done"/>')
        mock_agent.stop = mocker.AsyncMock()

        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        full_ticket = await state_manager.get_ticket(ticket.id)
        assert full_ticket is not None
        await scheduler._handle_complete(full_ticket)

        # Ticket should be in REVIEW (not DONE)
        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.REVIEW
        assert updated.checks_passed is True

        # Merge should NOT have been called
        mock_worktree_manager.merge_to_main.assert_not_called()

    async def test_no_auto_merge_when_review_rejected(
        self,
        auto_merge_scheduler,
        state_manager: StateManager,
        mock_worktree_manager,
        mocker,
    ):
        """Test no auto-merge when review is rejected."""
        ticket = await state_manager.create_ticket(
            TicketCreate(
                title="Auto ticket",
                ticket_type=TicketType.AUTO,
                status=TicketStatus.IN_PROGRESS,
            )
        )

        # Mock review agent returning reject signal
        mock_agent = mocker.MagicMock()
        mock_agent.set_auto_approve = mocker.MagicMock()
        mock_agent.start = mocker.MagicMock()
        mock_agent.wait_ready = mocker.AsyncMock()
        mock_agent.send_prompt = mocker.AsyncMock()
        mock_agent.get_response_text = mocker.MagicMock(
            return_value='Missing tests. <reject reason="No unit tests added"/>'
        )
        mock_agent.stop = mocker.AsyncMock()

        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        full_ticket = await state_manager.get_ticket(ticket.id)
        assert full_ticket is not None
        await auto_merge_scheduler._handle_complete(full_ticket)

        # Ticket should stay in REVIEW
        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.REVIEW
        assert updated.checks_passed is False
        assert updated.review_summary == "No unit tests added"

        # Merge should NOT have been called
        mock_worktree_manager.merge_to_main.assert_not_called()
