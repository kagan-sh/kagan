"""Tests for scheduler with mock ACP agent.

Integration tests that verify scheduler behavior with mocked external dependencies.
Unit tests for scheduler internals are in tests/unit/test_scheduler.py.
"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

import pytest

from kagan.database.models import Ticket, TicketStatus, TicketType

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
        assert len(scheduler._running) == 0

    async def test_tick_with_no_tickets(self, scheduler: Scheduler):
        """Test scheduler does nothing with no tickets when started."""
        scheduler.start()
        await asyncio.sleep(0.1)
        assert len(scheduler._running_tickets) == 0
        await scheduler.stop()

    @pytest.mark.parametrize(
        "ticket_type,status,expected_running",
        [
            (TicketType.PAIR, TicketStatus.IN_PROGRESS, 0),
            (TicketType.AUTO, TicketStatus.BACKLOG, 0),
        ],
        ids=["pair_ignored", "backlog_ignored"],
    )
    async def test_scheduler_ignores_ineligible_tickets(
        self,
        scheduler: Scheduler,
        state_manager: StateManager,
        ticket_type: TicketType,
        status: TicketStatus,
        expected_running: int,
    ):
        """Test scheduler ignores PAIR tickets and AUTO tickets in BACKLOG."""
        scheduler.start()
        ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Test ticket",
                ticket_type=ticket_type,
                status=status,
            )
        )
        await scheduler.handle_status_change(ticket.id, None, status)
        await asyncio.sleep(0.1)
        assert len(scheduler._running_tickets) == expected_running
        await scheduler.stop()


class TestSchedulerWithMockAgent:
    """Scheduler tests with mocked ACP agent."""

    async def test_scheduler_identifies_auto_tickets(
        self, scheduler: Scheduler, state_manager: StateManager
    ):
        """Test scheduler correctly identifies AUTO tickets to process."""
        auto_ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Auto ticket",
                ticket_type=TicketType.AUTO,
                status=TicketStatus.IN_PROGRESS,
            )
        )
        await state_manager.create_ticket(
            Ticket.create(
                title="Pair ticket",
                ticket_type=TicketType.PAIR,
                status=TicketStatus.IN_PROGRESS,
            )
        )

        tickets = await state_manager.get_all_tickets()
        eligible = [
            t
            for t in tickets
            if t.status == TicketStatus.IN_PROGRESS and t.ticket_type == TicketType.AUTO
        ]

        assert len(eligible) == 1
        assert eligible[0].id == auto_ticket.id

    @pytest.mark.parametrize(
        "response,expected_status",
        [
            ('<blocked reason="Need help"/>', TicketStatus.BACKLOG),
            ("Still working... <continue/>", TicketStatus.BACKLOG),  # max_iterations=3
        ],
        ids=["blocked_signal", "max_iterations"],
    )
    async def test_scheduler_status_transitions(
        self,
        scheduler: Scheduler,
        state_manager: StateManager,
        mock_agent,
        mocker,
        response: str,
        expected_status: TicketStatus,
    ):
        """Test scheduler moves ticket to expected status based on agent response."""
        ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Auto ticket",
                ticket_type=TicketType.AUTO,
                status=TicketStatus.IN_PROGRESS,
            )
        )

        mock_agent.get_response_text.return_value = response
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        scheduler.start()
        await scheduler.handle_status_change(ticket.id, None, TicketStatus.IN_PROGRESS)

        for _ in range(50):
            await asyncio.sleep(0.1)
            updated = await state_manager.get_ticket(ticket.id)
            if updated and updated.status == expected_status:
                break

        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == expected_status
        await scheduler.stop()

    async def test_get_agent_config_priority(
        self, scheduler: Scheduler, state_manager: StateManager
    ):
        """Test agent config selection priority."""
        ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Test",
                ticket_type=TicketType.AUTO,
                agent_backend="test",
            )
        )
        full_ticket = await state_manager.get_ticket(ticket.id)
        assert full_ticket is not None

        config = scheduler._get_agent_config(full_ticket)
        assert config is not None
        assert config.short_name == "test"
