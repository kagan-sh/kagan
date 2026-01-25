"""Tests for Scheduler class."""

from __future__ import annotations

import asyncio
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from kagan.agents.process import AgentState
from kagan.agents.scheduler import Scheduler
from kagan.config import GeneralConfig, HatConfig, KaganConfig
from kagan.database.models import Ticket, TicketStatus, TicketUpdate


@pytest.fixture
def mock_state():
    m = MagicMock()
    m.get_all_tickets = AsyncMock(return_value=[])
    m.update_ticket = AsyncMock()
    m.get_scratchpad = AsyncMock(return_value="")
    m.update_scratchpad = AsyncMock()
    m.delete_scratchpad = AsyncMock()
    m.add_knowledge = AsyncMock()
    return m


@pytest.fixture
def mock_agents():
    m = MagicMock()
    m.get = MagicMock(return_value=None)
    m.list_active = MagicMock(return_value=[])
    m.spawn = AsyncMock()
    m.terminate = AsyncMock()
    return m


@pytest.fixture
def mock_worktrees():
    m = MagicMock()
    m.get_path = AsyncMock(return_value=None)
    m.create = AsyncMock(return_value=Path("/tmp/wt"))
    return m


@pytest.fixture
def config():
    return KaganConfig(
        general=GeneralConfig(auto_start=True, max_concurrent_agents=2),
        hats={"dev": HatConfig(agent_command="claude", args=["--model", "opus"])},
    )


class TestScheduler:
    async def test_handles_finished_agent(self, mock_state, mock_agents, mock_worktrees, config):
        """FINISHED agents move tickets to REVIEW."""
        ticket = Ticket(id="t1", title="Test", status=TicketStatus.IN_PROGRESS)
        mock_state.get_all_tickets.return_value = [ticket]
        mock_agents.get.return_value = MagicMock(state=AgentState.FINISHED)

        await Scheduler(mock_state, mock_agents, mock_worktrees, config).tick()

        mock_state.update_ticket.assert_called_once_with(
            "t1", TicketUpdate(status=TicketStatus.REVIEW)
        )
        mock_agents.terminate.assert_called_once_with("t1")

    async def test_handles_failed_agent(self, mock_state, mock_agents, mock_worktrees, config):
        """FAILED agents move tickets to BACKLOG."""
        ticket = Ticket(id="t2", title="Test", status=TicketStatus.IN_PROGRESS)
        mock_state.get_all_tickets.return_value = [ticket]
        mock_agents.get.return_value = MagicMock(state=AgentState.FAILED)

        await Scheduler(mock_state, mock_agents, mock_worktrees, config).tick()

        mock_state.update_ticket.assert_called_once_with(
            "t2", TicketUpdate(status=TicketStatus.BACKLOG)
        )

    async def test_respects_auto_start_false(self, mock_state, mock_agents, mock_worktrees):
        """No agents spawned when auto_start=False."""
        config = KaganConfig(general=GeneralConfig(auto_start=False))
        ticket = Ticket(id="t3", title="Test", status=TicketStatus.IN_PROGRESS)
        mock_state.get_all_tickets.return_value = [ticket]

        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)
        await scheduler.tick()

        # No tasks should be created with auto_start=False
        assert len(scheduler._running_tickets) == 0

    async def test_respects_max_concurrent(self, mock_state, mock_agents, mock_worktrees):
        """Respects max_concurrent_agents limit."""
        config = KaganConfig(general=GeneralConfig(auto_start=True, max_concurrent_agents=1))
        tickets = [
            Ticket(id="t1", title="Test1", status=TicketStatus.IN_PROGRESS),
            Ticket(id="t2", title="Test2", status=TicketStatus.IN_PROGRESS),
        ]
        mock_state.get_all_tickets.return_value = tickets

        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)

        # Patch _run_ticket_loop to avoid actual execution
        with patch.object(scheduler, "_run_ticket_loop", new_callable=AsyncMock):
            await scheduler.tick()
            # Only one ticket should be running due to max_concurrent=1
            assert len(scheduler._running_tickets) == 1

    async def test_starts_ticket_loop(self, mock_state, mock_agents, mock_worktrees, config):
        """Starts iterative loop for IN_PROGRESS ticket."""
        ticket = Ticket(id="t5", title="Feature", status=TicketStatus.IN_PROGRESS)
        mock_state.get_all_tickets.return_value = [ticket]

        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)

        with patch.object(scheduler, "_run_ticket_loop", new_callable=AsyncMock) as mock_loop:
            await scheduler.tick()
            # Give time for task to start
            await asyncio.sleep(0.01)
            mock_loop.assert_called_once_with(ticket)
            assert "t5" in scheduler._running_tickets

    async def test_get_command_with_hat(self, mock_state, mock_agents, mock_worktrees, config):
        """Gets command from hat configuration."""
        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)
        cmd = scheduler._get_command("dev")
        assert cmd == "claude --model opus"

    async def test_get_command_fallback(self, mock_state, mock_agents, mock_worktrees):
        """Falls back to 'claude' when no hats configured."""
        config = KaganConfig(general=GeneralConfig(auto_start=True), hats={})
        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)
        cmd = scheduler._get_command(None)
        assert cmd == "claude"

    async def test_get_iteration(self, mock_state, mock_agents, mock_worktrees, config):
        """Returns current iteration count for ticket."""
        scheduler = Scheduler(mock_state, mock_agents, mock_worktrees, config)
        scheduler._iteration_counts["t1"] = 3
        assert scheduler.get_iteration("t1") == 3
        assert scheduler.get_iteration("unknown") is None
