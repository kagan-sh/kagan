"""Tests for scheduler basics with mock ACP agent."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.database.models import TicketCreate, TicketStatus, TicketType

if TYPE_CHECKING:
    from kagan.agents.scheduler import Scheduler
    from kagan.database.manager import StateManager

pytestmark = pytest.mark.integration


class TestSchedulerBasics:
    """Basic scheduler tests."""

    async def test_scheduler_initialization(self, scheduler: Scheduler):
        """Test scheduler initializes correctly."""
        assert scheduler is not None
        assert len(scheduler._running_tickets) == 0
        assert len(scheduler._agents) == 0

    async def test_tick_with_no_tickets(self, scheduler: Scheduler):
        """Test tick does nothing with no tickets."""
        await scheduler.tick()
        assert len(scheduler._running_tickets) == 0

    async def test_tick_ignores_pair_tickets(
        self, scheduler: Scheduler, state_manager: StateManager
    ):
        """Test tick ignores PAIR mode tickets."""
        # Create a PAIR ticket in IN_PROGRESS
        await state_manager.create_ticket(
            TicketCreate(
                title="Pair ticket",
                ticket_type=TicketType.PAIR,
                status=TicketStatus.IN_PROGRESS,
            )
        )

        await scheduler.tick()

        # PAIR tickets should not be picked up
        assert len(scheduler._running_tickets) == 0

    async def test_tick_ignores_backlog_auto_tickets(
        self, scheduler: Scheduler, state_manager: StateManager
    ):
        """Test tick ignores AUTO tickets in BACKLOG."""
        await state_manager.create_ticket(
            TicketCreate(
                title="Auto backlog",
                ticket_type=TicketType.AUTO,
                status=TicketStatus.BACKLOG,
            )
        )

        await scheduler.tick()

        # Backlog tickets should not be picked up
        assert len(scheduler._running_tickets) == 0


class TestSchedulerHelpers:
    """Tests for scheduler helper methods."""

    async def test_is_running(self, scheduler: Scheduler):
        """Test is_running method."""
        assert not scheduler.is_running("test-id")
        scheduler._running_tickets.add("test-id")
        assert scheduler.is_running("test-id")

    async def test_get_running_agent(self, scheduler: Scheduler, mocker):
        """Test get_running_agent method."""
        assert scheduler.get_running_agent("test-id") is None
        mock_agent = mocker.MagicMock()
        scheduler._agents["test-id"] = mock_agent
        assert scheduler.get_running_agent("test-id") is mock_agent

    async def test_get_iteration_count(self, scheduler: Scheduler):
        """Test get_iteration_count method."""
        assert scheduler.get_iteration_count("test-id") == 0
        scheduler._iteration_counts["test-id"] = 5
        assert scheduler.get_iteration_count("test-id") == 5

    async def test_stop(self, scheduler: Scheduler, mocker):
        """Test stop method cleans up."""
        mock_agent = mocker.MagicMock()
        mock_agent.stop = mocker.AsyncMock()
        scheduler._agents["test-id"] = mock_agent
        scheduler._running_tickets.add("test-id")
        scheduler._iteration_counts["test-id"] = 3

        await scheduler.stop()

        assert len(scheduler._agents) == 0
        assert len(scheduler._running_tickets) == 0
        assert len(scheduler._iteration_counts) == 0
        mock_agent.stop.assert_called_once()
