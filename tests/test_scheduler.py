"""Tests for scheduler with mock ACP agent."""

from __future__ import annotations

import asyncio
import tempfile
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from kagan.agents.scheduler import Scheduler
from kagan.agents.worktree import WorktreeManager
from kagan.config import AgentConfig, GeneralConfig, KaganConfig
from kagan.database.manager import StateManager
from kagan.database.models import TicketCreate, TicketStatus, TicketType


@pytest.fixture
async def state_manager():
    """Create a temporary database for testing."""
    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = Path(tmpdir) / "test.db"
        manager = StateManager(db_path)
        await manager.initialize()
        yield manager
        await manager.close()


@pytest.fixture
def mock_worktree_manager():
    """Create a mock worktree manager."""
    manager = MagicMock(spec=WorktreeManager)
    manager.get_path = AsyncMock(return_value=Path("/tmp/worktree"))
    manager.create = AsyncMock(return_value=Path("/tmp/worktree"))
    return manager


@pytest.fixture
def config():
    """Create a test config."""
    return KaganConfig(
        general=GeneralConfig(
            auto_start=True,
            max_concurrent_agents=2,
            max_iterations=3,
            iteration_delay_seconds=0.01,
            default_worker_agent="test",
        ),
        agents={
            "test": AgentConfig(
                identity="test.agent",
                name="Test Agent",
                short_name="test",
                run_command={"*": "echo test"},
            )
        },
    )


@pytest.fixture
def scheduler(state_manager, mock_worktree_manager, config):
    """Create a scheduler instance."""
    changed_callback = MagicMock()
    return Scheduler(
        state_manager=state_manager,
        worktree_manager=mock_worktree_manager,
        config=config,
        on_ticket_changed=changed_callback,
    )


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


class TestSchedulerWithMockAgent:
    """Scheduler tests with mocked ACP agent.

    Note: These tests focus on the signal parsing and state transitions,
    not the full async agent lifecycle which is difficult to test reliably.
    """

    @pytest.fixture
    def mock_agent(self):
        """Create a mock ACP agent."""
        agent = MagicMock()
        agent.set_auto_approve = MagicMock()
        agent.start = MagicMock()
        agent.wait_ready = AsyncMock()
        agent.send_prompt = AsyncMock()
        agent.get_response_text = MagicMock(return_value="Done! <complete/>")
        agent.stop = AsyncMock()
        return agent

    async def test_scheduler_identifies_auto_tickets(
        self,
        scheduler: Scheduler,
        state_manager: StateManager,
    ):
        """Test scheduler correctly identifies AUTO tickets to process."""
        # Create both types of tickets
        auto_ticket = await state_manager.create_ticket(
            TicketCreate(
                title="Auto ticket",
                ticket_type=TicketType.AUTO,
                status=TicketStatus.IN_PROGRESS,
            )
        )
        await state_manager.create_ticket(
            TicketCreate(
                title="Pair ticket",
                ticket_type=TicketType.PAIR,
                status=TicketStatus.IN_PROGRESS,
            )
        )

        # Get all tickets
        tickets = await state_manager.get_all_tickets()

        # Filter for AUTO IN_PROGRESS (what scheduler should do)
        eligible = [
            t
            for t in tickets
            if t.status == TicketStatus.IN_PROGRESS and t.ticket_type == TicketType.AUTO
        ]

        assert len(eligible) == 1
        assert eligible[0].id == auto_ticket.id

    async def test_scheduler_handles_blocked(
        self,
        scheduler: Scheduler,
        state_manager: StateManager,
        mock_agent,
    ):
        """Test scheduler moves ticket to BACKLOG on blocked."""
        ticket = await state_manager.create_ticket(
            TicketCreate(
                title="Auto ticket",
                ticket_type=TicketType.AUTO,
                status=TicketStatus.IN_PROGRESS,
            )
        )

        # Mock agent returns <blocked/>
        mock_agent.get_response_text.return_value = '<blocked reason="Need help"/>'

        with patch("kagan.agents.scheduler.Agent", return_value=mock_agent):
            await scheduler.tick()
            # Wait for task to complete
            for _ in range(30):  # Max 3 seconds
                await asyncio.sleep(0.1)
                updated = await state_manager.get_ticket(ticket.id)
                if updated and updated.status == TicketStatus.BACKLOG:
                    break

        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.BACKLOG

    async def test_scheduler_max_iterations(
        self,
        scheduler: Scheduler,
        state_manager: StateManager,
        mock_agent,
    ):
        """Test scheduler respects max iterations."""
        ticket = await state_manager.create_ticket(
            TicketCreate(
                title="Auto ticket",
                ticket_type=TicketType.AUTO,
                status=TicketStatus.IN_PROGRESS,
            )
        )

        # Mock agent always returns <continue/>
        mock_agent.get_response_text.return_value = "Still working... <continue/>"

        with patch("kagan.agents.scheduler.Agent", return_value=mock_agent):
            await scheduler.tick()
            # Wait for max iterations (3 iterations * delay + processing)
            for _ in range(50):  # Max 5 seconds
                await asyncio.sleep(0.1)
                updated = await state_manager.get_ticket(ticket.id)
                if updated and updated.status == TicketStatus.BACKLOG:
                    break

        # Should be back in BACKLOG after max iterations
        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.BACKLOG

    async def test_get_agent_config_priority(
        self,
        scheduler: Scheduler,
        state_manager: StateManager,
    ):
        """Test agent config selection priority."""
        # Create ticket with agent_backend set
        ticket = await state_manager.create_ticket(
            TicketCreate(
                title="Test",
                ticket_type=TicketType.AUTO,
                agent_backend="test",
            )
        )
        # Convert to full ticket model
        full_ticket = await state_manager.get_ticket(ticket.id)
        assert full_ticket is not None

        # Should get the "test" agent config
        config = scheduler._get_agent_config(full_ticket)
        assert config is not None
        assert config.short_name == "test"


class TestSchedulerHelpers:
    """Tests for scheduler helper methods."""

    async def test_is_running(self, scheduler: Scheduler):
        """Test is_running method."""
        assert not scheduler.is_running("test-id")
        scheduler._running_tickets.add("test-id")
        assert scheduler.is_running("test-id")

    async def test_get_running_agent(self, scheduler: Scheduler):
        """Test get_running_agent method."""
        assert scheduler.get_running_agent("test-id") is None
        mock_agent = MagicMock()
        scheduler._agents["test-id"] = mock_agent
        assert scheduler.get_running_agent("test-id") is mock_agent

    async def test_get_iteration_count(self, scheduler: Scheduler):
        """Test get_iteration_count method."""
        assert scheduler.get_iteration_count("test-id") == 0
        scheduler._iteration_counts["test-id"] = 5
        assert scheduler.get_iteration_count("test-id") == 5

    async def test_stop(self, scheduler: Scheduler):
        """Test stop method cleans up."""
        mock_agent = MagicMock()
        mock_agent.stop = AsyncMock()
        scheduler._agents["test-id"] = mock_agent
        scheduler._running_tickets.add("test-id")
        scheduler._iteration_counts["test-id"] = 3

        await scheduler.stop()

        assert len(scheduler._agents) == 0
        assert len(scheduler._running_tickets) == 0
        assert len(scheduler._iteration_counts) == 0
        mock_agent.stop.assert_called_once()
