"""Tests for auto-merge functionality with agent-based review."""

from __future__ import annotations

from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock

import pytest

from kagan.agents.scheduler import Scheduler
from kagan.database.models import Ticket, TicketStatus, TicketType

if TYPE_CHECKING:
    from kagan.database.manager import StateManager

pytestmark = pytest.mark.integration


def _create_review_agent(response: str) -> MagicMock:
    """Create a mock review agent with specified response."""
    agent = MagicMock()
    agent.set_auto_approve = MagicMock()
    agent.start = MagicMock()
    agent.wait_ready = AsyncMock()
    agent.send_prompt = AsyncMock()
    agent.get_response_text = MagicMock(return_value=response)
    agent.stop = AsyncMock()
    return agent


@pytest.fixture
def auto_merge_scheduler(
    state_manager, mock_worktree_manager, auto_merge_config, mock_session_manager, mocker
):
    """Create a scheduler with auto_merge enabled."""
    mock_worktree_manager.get_commit_log = mocker.AsyncMock(return_value=["feat: add feature"])
    mock_worktree_manager.get_diff_stats = mocker.AsyncMock(return_value="1 file changed")
    return Scheduler(
        state_manager=state_manager,
        worktree_manager=mock_worktree_manager,
        config=auto_merge_config,
        session_manager=mock_session_manager,
        on_ticket_changed=mocker.MagicMock(),
    )


async def _create_auto_ticket(state_manager: StateManager) -> Ticket:
    """Create a standard AUTO ticket in IN_PROGRESS status."""
    return await state_manager.create_ticket(
        Ticket.create(
            title="Auto ticket",
            ticket_type=TicketType.AUTO,
            status=TicketStatus.IN_PROGRESS,
        )
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
        ticket = await _create_auto_ticket(state_manager)

        mock_worktree_manager.merge_to_main = mocker.AsyncMock(return_value=(True, "Merged"))
        mock_worktree_manager.delete = mocker.AsyncMock()
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_review_agent)

        full_ticket = await state_manager.get_ticket(ticket.id)
        await auto_merge_scheduler._handle_complete(full_ticket)

        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.DONE
        assert updated.checks_passed is True
        assert updated.review_summary == "Implementation complete"

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
        ticket = await _create_auto_ticket(state_manager)

        mock_worktree_manager.get_commit_log = mocker.AsyncMock(return_value=["feat: add feature"])
        mock_worktree_manager.get_diff_stats = mocker.AsyncMock(return_value="1 file changed")
        mocker.patch(
            "kagan.agents.scheduler.Agent",
            return_value=_create_review_agent('<approve summary="Done"/>'),
        )

        full_ticket = await state_manager.get_ticket(ticket.id)
        await scheduler._handle_complete(full_ticket)

        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.REVIEW
        assert updated.checks_passed is True
        mock_worktree_manager.merge_to_main.assert_not_called()

    @pytest.mark.parametrize(
        "response,expected_passed,expected_summary_contains",
        [
            ('<reject reason="No unit tests added"/>', False, "No unit tests added"),
            ("The code looks fine but I need more context.", False, "No review signal found"),
        ],
        ids=["rejected", "no_signal"],
    )
    async def test_no_auto_merge_on_review_issues(
        self,
        auto_merge_scheduler,
        state_manager: StateManager,
        mock_worktree_manager,
        mocker,
        response,
        expected_passed,
        expected_summary_contains,
    ):
        """Test no auto-merge when review is rejected or has no signal."""
        ticket = await _create_auto_ticket(state_manager)

        mocker.patch("kagan.agents.scheduler.Agent", return_value=_create_review_agent(response))

        full_ticket = await state_manager.get_ticket(ticket.id)
        await auto_merge_scheduler._handle_complete(full_ticket)

        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.REVIEW
        assert updated.checks_passed is expected_passed
        assert expected_summary_contains in (updated.review_summary or "")
        mock_worktree_manager.merge_to_main.assert_not_called()


class TestAutoMergeEdgeCases:
    """Edge cases for auto-merge functionality."""

    async def test_stays_in_review_when_merge_fails(
        self,
        auto_merge_scheduler,
        state_manager: StateManager,
        mock_worktree_manager,
        mock_review_agent,
        mocker,
    ):
        """Test ticket stays in REVIEW if merge fails."""
        ticket = await _create_auto_ticket(state_manager)

        mock_worktree_manager.merge_to_main = mocker.AsyncMock(
            return_value=(False, "Merge conflict")
        )
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_review_agent)

        full_ticket = await state_manager.get_ticket(ticket.id)
        await auto_merge_scheduler._handle_complete(full_ticket)

        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.REVIEW
        mock_worktree_manager.delete.assert_not_called()

    @pytest.mark.parametrize(
        "response,expected_passed,expected_summary",
        [
            ('<approve summary="All good"/>', True, "All good"),
            ('<reject reason="Needs work"/>', False, "Needs work"),
        ],
        ids=["approve", "reject"],
    )
    async def test_run_review_helper(
        self,
        auto_merge_scheduler,
        state_manager: StateManager,
        mocker,
        response,
        expected_passed,
        expected_summary,
    ):
        """Test _run_review helper method parses signals correctly."""
        from pathlib import Path

        ticket = await state_manager.create_ticket(
            Ticket.create(title="Test ticket", ticket_type=TicketType.AUTO, description="Test")
        )
        full_ticket = await state_manager.get_ticket(ticket.id)
        wt_path = Path("/tmp/test-worktree")

        mocker.patch("kagan.agents.scheduler.Agent", return_value=_create_review_agent(response))

        passed, summary = await auto_merge_scheduler._run_review(full_ticket, wt_path)
        assert passed is expected_passed
        assert summary == expected_summary
