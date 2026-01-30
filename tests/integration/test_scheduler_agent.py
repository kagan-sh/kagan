"""Tests for scheduler with mock ACP agent."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

import pytest

from kagan.database.models import TicketCreate, TicketStatus, TicketType

if TYPE_CHECKING:
    from kagan.agents.scheduler import Scheduler
    from kagan.database.manager import StateManager

pytestmark = pytest.mark.integration


class TestSchedulerWithMockAgent:
    """Scheduler tests with mocked ACP agent."""

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
        mocker,
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

        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

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
        mocker,
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

        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

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
