"""Unit tests for reactive Scheduler."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock

import pytest

from kagan.agents.scheduler import RunningTicketState, Scheduler
from kagan.database.models import Ticket, TicketStatus, TicketType
from tests.helpers.mocks import create_test_config

if TYPE_CHECKING:
    from kagan.database.manager import StateManager

pytestmark = pytest.mark.unit


async def wait_for_status(sm: StateManager, tid: str, status: TicketStatus, timeout: float = 3.0):
    """Wait for ticket to reach expected status."""
    deadline = asyncio.get_running_loop().time() + timeout
    while asyncio.get_running_loop().time() < deadline:
        if (t := await sm.get_ticket(tid)) and t.status == status:
            return t
        await asyncio.sleep(0.05)
    return None


async def wait_for_running(scheduler: Scheduler, tid: str, timeout: float = 2.0):
    """Wait for ticket to appear in _running."""
    deadline = asyncio.get_running_loop().time() + timeout
    while asyncio.get_running_loop().time() < deadline:
        if tid in scheduler._running:
            return True
        await asyncio.sleep(0.05)
    return False


@pytest.fixture
def callbacks():
    return {"iter": MagicMock(), "ticket": MagicMock()}


@pytest.fixture
def scheduler_cb(state_manager, mock_worktree_manager, config, callbacks):
    """Create scheduler with callbacks (tests must call start())."""
    return Scheduler(
        state_manager=state_manager,
        worktree_manager=mock_worktree_manager,
        config=config,
        on_ticket_changed=callbacks["ticket"],
        on_iteration_changed=callbacks["iter"],
    )


class TestSchedulerLifecycle:
    async def test_state_queries_and_stop(self, scheduler: Scheduler):
        """Scheduler initializes empty, queries work, stop clears state."""
        scheduler.start()
        assert (
            len(scheduler._running),
            scheduler.is_running("x"),
            scheduler.get_running_agent("x"),
        ) == (0, False, None)
        assert scheduler.get_iteration_count("x") == 0

        agents = [MagicMock(stop=AsyncMock()), None]
        tasks = [
            MagicMock(spec=asyncio.Task, cancel=MagicMock(), done=MagicMock(return_value=False))
            for _ in range(2)
        ]
        scheduler._running["t1"] = RunningTicketState(task=tasks[0], agent=agents[0], iteration=5)
        scheduler._running["t2"] = RunningTicketState(task=tasks[1], agent=agents[1])

        assert scheduler.is_running("t1") and scheduler.get_running_agent("t1") is agents[0]
        assert scheduler.get_iteration_count("t1") == 5

        await scheduler.stop()
        assert len(scheduler._running) == 0
        agents[0].stop.assert_called_once()  # type: ignore[union-attr]
        for t in tasks:
            t.cancel.assert_called_once()

    async def test_stop_ticket(self, scheduler_cb: Scheduler, callbacks):
        """Stop ticket via queue: not running returns False; running cleans up."""
        scheduler_cb.start()
        # Not running
        assert await scheduler_cb.stop_ticket("x") is False

        # Running
        mock_agent = MagicMock(stop=AsyncMock())
        task = asyncio.create_task(asyncio.sleep(100))
        scheduler_cb._running["t1"] = RunningTicketState(task=task, agent=mock_agent, iteration=5)

        assert await scheduler_cb.stop_ticket("t1") is True
        # Give worker time to process
        await asyncio.sleep(0.2)
        assert "t1" not in scheduler_cb._running
        mock_agent.stop.assert_called_once()
        await scheduler_cb.stop()


class TestReactiveSpawn:
    async def test_status_change_spawns_agent(
        self, state_manager: StateManager, mock_worktree_manager, mock_agent, mocker
    ):
        """Moving ticket to IN_PROGRESS spawns agent."""
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)
        mock_agent.get_response_text.return_value = "<continue/>"

        sched = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=create_test_config(max_iterations=100),
        )
        sched.start()

        ticket = await state_manager.create_ticket(
            Ticket.create(title="T", ticket_type=TicketType.AUTO, status=TicketStatus.BACKLOG)
        )

        # Trigger status change
        await sched.handle_status_change(ticket.id, TicketStatus.BACKLOG, TicketStatus.IN_PROGRESS)

        # Wait for spawn
        assert await wait_for_running(sched, ticket.id)
        await sched.stop()

    async def test_status_change_stops_agent(
        self, state_manager: StateManager, mock_worktree_manager, mock_agent, mocker
    ):
        """Moving ticket out of IN_PROGRESS stops agent."""
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)
        mock_agent.get_response_text.return_value = "<continue/>"

        sched = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=create_test_config(max_iterations=100),
        )
        sched.start()

        ticket = await state_manager.create_ticket(
            Ticket.create(title="T", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS)
        )

        # Spawn
        await sched.handle_status_change(ticket.id, TicketStatus.BACKLOG, TicketStatus.IN_PROGRESS)
        assert await wait_for_running(sched, ticket.id)

        # Stop via status change
        await sched.handle_status_change(ticket.id, TicketStatus.IN_PROGRESS, TicketStatus.BACKLOG)
        await asyncio.sleep(0.2)
        assert ticket.id not in sched._running
        await sched.stop()

    async def test_respects_capacity(
        self, state_manager: StateManager, mock_worktree_manager, mock_agent, mocker
    ):
        """Scheduler respects max_concurrent_agents limit."""
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)
        mock_agent.get_response_text.return_value = "<continue/>"

        sched = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=create_test_config(max_concurrent=2, max_iterations=100),
        )
        sched.start()

        tickets = [
            await state_manager.create_ticket(
                Ticket.create(
                    title=f"T{i}", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS
                )
            )
            for i in range(4)
        ]

        # Trigger all at once
        for t in tickets:
            await sched.handle_status_change(t.id, TicketStatus.BACKLOG, TicketStatus.IN_PROGRESS)

        await asyncio.sleep(0.3)
        assert len(sched._running) <= 2
        await sched.stop()

    async def test_ignores_pair_tickets(
        self, state_manager: StateManager, mock_worktree_manager, mock_agent, mocker
    ):
        """Scheduler ignores PAIR tickets."""
        factory = mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        sched = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=create_test_config(),
        )
        sched.start()

        ticket = await state_manager.create_ticket(
            Ticket.create(title="T", ticket_type=TicketType.PAIR, status=TicketStatus.IN_PROGRESS)
        )

        await sched.handle_status_change(ticket.id, TicketStatus.BACKLOG, TicketStatus.IN_PROGRESS)
        await asyncio.sleep(0.2)

        assert ticket.id not in sched._running
        factory.assert_not_called()
        await sched.stop()


class TestAgentSignalsAndErrors:
    @pytest.mark.parametrize(
        "response,expected",
        [
            ("Done! <complete/>", TicketStatus.REVIEW),
            ('<blocked reason="Help!"/>', TicketStatus.BACKLOG),
        ],
    )
    async def test_signal_moves_ticket(
        self,
        state_manager: StateManager,
        mock_worktree_manager,
        mock_agent,
        mocker,
        response,
        expected,
    ):
        """Agent signals move ticket to correct status."""
        mock_agent.get_response_text.return_value = response
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        sched = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=create_test_config(),
        )
        sched.start()

        ticket = await state_manager.create_ticket(
            Ticket.create(title="T", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS)
        )

        await sched.handle_status_change(ticket.id, TicketStatus.BACKLOG, TicketStatus.IN_PROGRESS)
        assert (await wait_for_status(state_manager, ticket.id, expected)) is not None
        await sched.stop()

    async def test_max_iterations(
        self, state_manager: StateManager, mock_worktree_manager, mock_agent, mocker
    ):
        """Max iterations moves to BACKLOG; iteration callback called."""
        iter_cb = MagicMock()
        mock_agent.get_response_text.return_value = "<continue/>"
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        sched = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=create_test_config(max_iterations=2),
            on_iteration_changed=iter_cb,
        )
        sched.start()

        ticket = await state_manager.create_ticket(
            Ticket.create(title="T", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS)
        )

        await sched.handle_status_change(ticket.id, TicketStatus.BACKLOG, TicketStatus.IN_PROGRESS)
        assert (
            await wait_for_status(state_manager, ticket.id, TicketStatus.BACKLOG, timeout=5.0)
        ) is not None
        assert any(c[0] == (ticket.id, 1) for c in iter_cb.call_args_list)
        await sched.stop()

    @pytest.mark.parametrize("error_setup", ["exception", "timeout", "worktree"])
    async def test_error_moves_to_backlog(
        self,
        state_manager: StateManager,
        mock_worktree_manager,
        mock_agent,
        mocker,
        error_setup,
    ):
        """Various errors move ticket to BACKLOG."""
        if error_setup == "exception":
            mock_agent.send_prompt = AsyncMock(side_effect=RuntimeError("crash"))
        elif error_setup == "timeout":
            mock_agent.wait_ready = AsyncMock(side_effect=TimeoutError("timeout"))
        else:
            mock_worktree_manager.get_path = AsyncMock(return_value=None)
            mock_worktree_manager.create = AsyncMock(side_effect=RuntimeError("git error"))

        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        sched = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=create_test_config(),
        )
        sched.start()

        ticket = await state_manager.create_ticket(
            Ticket.create(title="T", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS)
        )

        await sched.handle_status_change(ticket.id, TicketStatus.BACKLOG, TicketStatus.IN_PROGRESS)
        assert (await wait_for_status(state_manager, ticket.id, TicketStatus.BACKLOG)) is not None
        await sched.stop()


class TestCallbacks:
    async def test_ticket_changed_callback(
        self,
        scheduler_cb: Scheduler,
        callbacks,
        state_manager: StateManager,
        mock_agent,
        mocker,
    ):
        """Ticket changed callback called on complete."""
        scheduler_cb.start()
        mock_agent.get_response_text.return_value = "<complete/>"
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        ticket = await state_manager.create_ticket(
            Ticket.create(title="T", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS)
        )

        await scheduler_cb.handle_status_change(
            ticket.id, TicketStatus.BACKLOG, TicketStatus.IN_PROGRESS
        )

        for _ in range(30):
            await asyncio.sleep(0.1)
            if callbacks["ticket"].called:
                break
        assert callbacks["ticket"].called
        await scheduler_cb.stop()

    async def test_no_callback_when_none(
        self, state_manager: StateManager, mock_worktree_manager, config, mock_agent, mocker
    ):
        """No error when callbacks are None."""
        mock_agent.get_response_text.return_value = "<complete/>"
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        sched = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=config,
            on_ticket_changed=None,
            on_iteration_changed=None,
        )
        sched.start()

        ticket = await state_manager.create_ticket(
            Ticket.create(title="T", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS)
        )

        await sched.handle_status_change(ticket.id, TicketStatus.BACKLOG, TicketStatus.IN_PROGRESS)
        await wait_for_status(state_manager, ticket.id, TicketStatus.REVIEW)
        await sched.stop()
