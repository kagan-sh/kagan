"""Extended tests for auto-merge functionality - no signal, merge failure, and helper methods."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from kagan.database.models import TicketCreate, TicketStatus, TicketType

if TYPE_CHECKING:
    from kagan.database.manager import StateManager

pytestmark = pytest.mark.integration


@pytest.fixture
def auto_merge_scheduler(
    state_manager, mock_worktree_manager, auto_merge_config, mock_session_manager, mocker
):
    """Create a scheduler with auto_merge enabled."""
    from kagan.agents.scheduler import Scheduler

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


class TestAutoMergeExtended:
    """Extended tests for auto-merge - edge cases and helper methods."""

    async def test_no_auto_merge_when_no_signal(
        self,
        auto_merge_scheduler,
        state_manager: StateManager,
        mock_worktree_manager,
        mocker,
    ):
        """Test no auto-merge when review agent returns no signal."""
        ticket = await state_manager.create_ticket(
            TicketCreate(
                title="Auto ticket",
                ticket_type=TicketType.AUTO,
                status=TicketStatus.IN_PROGRESS,
            )
        )

        # Mock review agent returning no signal
        mock_agent = mocker.MagicMock()
        mock_agent.set_auto_approve = mocker.MagicMock()
        mock_agent.start = mocker.MagicMock()
        mock_agent.wait_ready = mocker.AsyncMock()
        mock_agent.send_prompt = mocker.AsyncMock()
        mock_agent.get_response_text = mocker.MagicMock(
            return_value="The code looks fine but I need more context."
        )
        mock_agent.stop = mocker.AsyncMock()

        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        full_ticket = await state_manager.get_ticket(ticket.id)
        assert full_ticket is not None
        await auto_merge_scheduler._handle_complete(full_ticket)

        # Ticket should stay in REVIEW with checks_passed=False
        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.REVIEW
        assert updated.checks_passed is False
        assert "No review signal found" in (updated.review_summary or "")

        # Merge should NOT have been called
        mock_worktree_manager.merge_to_main.assert_not_called()

    async def test_stays_in_review_when_merge_fails(
        self,
        auto_merge_scheduler,
        state_manager: StateManager,
        mock_worktree_manager,
        mock_review_agent,
        mocker,
    ):
        """Test ticket stays in REVIEW if merge fails."""
        ticket = await state_manager.create_ticket(
            TicketCreate(
                title="Auto ticket",
                ticket_type=TicketType.AUTO,
                status=TicketStatus.IN_PROGRESS,
            )
        )

        # Mock merge failure
        mock_worktree_manager.merge_to_main = mocker.AsyncMock(
            return_value=(False, "Merge conflict")
        )

        # Mock review agent returning approve signal
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_review_agent)

        full_ticket = await state_manager.get_ticket(ticket.id)
        assert full_ticket is not None
        await auto_merge_scheduler._handle_complete(full_ticket)

        # Ticket should stay in REVIEW (not moved to DONE)
        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.REVIEW  # Stays in REVIEW after failed merge

        # Cleanup should NOT have been called since merge failed
        mock_worktree_manager.delete.assert_not_called()

    async def test_run_review_helper(
        self,
        auto_merge_scheduler,
        state_manager: StateManager,
        mock_worktree_manager,
        mocker,
    ):
        """Test _run_review helper method."""
        ticket = await state_manager.create_ticket(
            TicketCreate(
                title="Test ticket",
                ticket_type=TicketType.AUTO,
                description="Test description",
            )
        )
        full_ticket = await state_manager.get_ticket(ticket.id)
        assert full_ticket is not None
        wt_path = Path("/tmp/test-worktree")

        # Test approve signal
        mock_agent = mocker.MagicMock()
        mock_agent.set_auto_approve = mocker.MagicMock()
        mock_agent.start = mocker.MagicMock()
        mock_agent.wait_ready = mocker.AsyncMock()
        mock_agent.send_prompt = mocker.AsyncMock()
        mock_agent.get_response_text = mocker.MagicMock(
            return_value='<approve summary="All good"/>'
        )
        mock_agent.stop = mocker.AsyncMock()

        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        passed, summary = await auto_merge_scheduler._run_review(full_ticket, wt_path)
        assert passed is True
        assert summary == "All good"

        # Test reject signal
        mock_agent.get_response_text.return_value = '<reject reason="Needs work"/>'

        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        passed, summary = await auto_merge_scheduler._run_review(full_ticket, wt_path)
        assert passed is False
        assert summary == "Needs work"
