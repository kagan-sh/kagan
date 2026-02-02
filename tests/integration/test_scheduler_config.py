"""Integration tests for config reload after settings changes.

Tests verify that config changes propagate correctly to scheduler and agents.
"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING
from unittest.mock import MagicMock

import pytest

from kagan.agents.scheduler import Scheduler
from kagan.config import AgentConfig, GeneralConfig, KaganConfig
from kagan.database.models import Ticket, TicketStatus, TicketType

if TYPE_CHECKING:
    from kagan.database.manager import StateManager

pytestmark = pytest.mark.integration


def _make_config(auto_approve: bool = False, max_concurrent: int = 2) -> KaganConfig:
    """Helper to create test config."""
    return KaganConfig(
        general=GeneralConfig(
            auto_start=True,
            auto_approve=auto_approve,
            max_concurrent_agents=max_concurrent,
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


class TestConfigObjectIdentity:
    """Tests for config object identity vs value propagation."""

    async def test_scheduler_shares_config_reference(
        self, state_manager: StateManager, mock_worktree_manager
    ):
        """Scheduler shares the same config object reference."""
        config = _make_config()
        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=config,
        )
        assert scheduler._config is config
        config.general.auto_approve = True
        assert scheduler._config.general.auto_approve is True


class TestAutoApproveOnRunningAgents:
    """Tests for auto_approve changes affecting running agents."""

    async def test_new_agent_gets_current_auto_approve(
        self, state_manager: StateManager, mock_worktree_manager, mock_agent, mocker
    ):
        """New agents receive current auto_approve value from config."""
        config = _make_config(auto_approve=True)

        set_auto_approve_calls = []
        mock_agent.set_auto_approve = MagicMock(
            side_effect=lambda v: set_auto_approve_calls.append(v)
        )
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=config,
        )

        ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Test", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS
            )
        )

        scheduler.start()
        await scheduler.handle_status_change(ticket.id, None, TicketStatus.IN_PROGRESS)
        for _ in range(20):
            await asyncio.sleep(0.05)
            if set_auto_approve_calls:
                break

        assert True in set_auto_approve_calls
        await scheduler.stop()

    async def test_running_agent_updated_on_config_change(
        self, state_manager: StateManager, mock_worktree_manager, mock_agent, mocker
    ):
        """Running agents update auto_approve when config changes between iterations."""
        config = _make_config(auto_approve=False)

        set_auto_approve_calls = []
        mock_agent.set_auto_approve = MagicMock(
            side_effect=lambda v: set_auto_approve_calls.append(v)
        )
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=config,
        )

        ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Test", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS
            )
        )

        scheduler.start()
        await scheduler.handle_status_change(ticket.id, None, TicketStatus.IN_PROGRESS)
        for _ in range(20):
            await asyncio.sleep(0.05)
            if set_auto_approve_calls:
                break

        assert False in set_auto_approve_calls

        config.general.auto_approve = True

        for _ in range(40):
            await asyncio.sleep(0.05)
            if True in set_auto_approve_calls:
                break

        assert False in set_auto_approve_calls
        assert True in set_auto_approve_calls
        await scheduler.stop()

    async def test_second_agent_gets_updated_config(
        self, state_manager: StateManager, mock_worktree_manager, mocker
    ):
        """Second agent spawned after config change gets new value."""
        config = _make_config(auto_approve=False)
        agent_auto_approve_values = []

        def mock_agent_factory(*args, **kwargs):
            agent = MagicMock()
            agent.set_auto_approve = MagicMock(
                side_effect=lambda v: agent_auto_approve_values.append(v)
            )
            agent.start = MagicMock()
            agent.wait_ready = mocker.AsyncMock()
            agent.send_prompt = mocker.AsyncMock()
            agent.get_response_text = MagicMock(return_value="<complete/>")
            agent.stop = mocker.AsyncMock()
            return agent

        mocker.patch("kagan.agents.scheduler.Agent", side_effect=mock_agent_factory)

        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=config,
        )

        ticket1 = await state_manager.create_ticket(
            Ticket.create(
                title="First", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS
            )
        )
        scheduler.start()
        await scheduler.handle_status_change(ticket1.id, None, TicketStatus.IN_PROGRESS)

        for _ in range(30):
            await asyncio.sleep(0.1)
            if agent_auto_approve_values:
                break

        assert False in agent_auto_approve_values

        config.general.auto_approve = True

        ticket2 = await state_manager.create_ticket(
            Ticket.create(
                title="Second", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS
            )
        )
        await scheduler.handle_status_change(ticket2.id, None, TicketStatus.IN_PROGRESS)

        for _ in range(30):
            await asyncio.sleep(0.1)
            if len(agent_auto_approve_values) >= 2:
                break

        assert True in agent_auto_approve_values
        await scheduler.stop()


class TestSchedulerConfigAccess:
    """Tests for scheduler config access patterns."""

    async def test_max_concurrent_agents_dynamic_update(
        self, state_manager: StateManager, mock_worktree_manager, mock_agent, mocker
    ):
        """Changing max_concurrent_agents affects spawning."""
        config = _make_config(max_concurrent=1)
        config.general.max_iterations = 100

        mock_agent.get_response_text.return_value = "<continue/>"
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=config,
        )

        tickets = []
        for i in range(3):
            ticket = await state_manager.create_ticket(
                Ticket.create(
                    title=f"Ticket {i}",
                    ticket_type=TicketType.AUTO,
                    status=TicketStatus.IN_PROGRESS,
                )
            )
            tickets.append(ticket)

        scheduler.start()
        # Queue first ticket
        await scheduler.handle_status_change(tickets[0].id, None, TicketStatus.IN_PROGRESS)
        await asyncio.sleep(0.1)
        assert len(scheduler._running_tickets) == 1

        config.general.max_concurrent_agents = 3
        # Queue remaining tickets
        for ticket in tickets[1:]:
            await scheduler.handle_status_change(ticket.id, None, TicketStatus.IN_PROGRESS)
        await asyncio.sleep(0.2)

        assert len(scheduler._running_tickets) >= 2
        await scheduler.stop()
