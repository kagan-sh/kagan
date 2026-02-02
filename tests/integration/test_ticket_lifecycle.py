"""Integration tests for ticket lifecycle from creation to completion."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.database.models import Ticket, TicketPriority, TicketStatus, TicketType

if TYPE_CHECKING:
    from kagan.agents.scheduler import Scheduler
    from kagan.database.manager import StateManager

pytestmark = pytest.mark.integration


class TestTicketLifecycle:
    """Core lifecycle: creation, transitions, review, completion."""

    async def test_create_ticket_starts_in_backlog(self, state_manager: StateManager):
        """Newly created tickets start in BACKLOG with correct fields."""
        ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Implement feature", description="Add auth", priority=TicketPriority.HIGH
            )
        )
        assert ticket.status == TicketStatus.BACKLOG
        assert ticket.title == "Implement feature"
        assert ticket.priority == TicketPriority.HIGH
        fetched = await state_manager.get_ticket(ticket.id)
        assert fetched is not None and fetched.status == TicketStatus.BACKLOG

    @pytest.mark.parametrize(
        "from_status,to_status",
        [
            (TicketStatus.BACKLOG, TicketStatus.IN_PROGRESS),
            (TicketStatus.IN_PROGRESS, TicketStatus.REVIEW),
            (TicketStatus.REVIEW, TicketStatus.DONE),
            (TicketStatus.BACKLOG, TicketStatus.DONE),  # skip
            (TicketStatus.DONE, TicketStatus.BACKLOG),  # backwards
            (TicketStatus.IN_PROGRESS, TicketStatus.IN_PROGRESS),  # idempotent
        ],
    )
    async def test_status_transitions(
        self, state_manager: StateManager, from_status: TicketStatus, to_status: TicketStatus
    ):
        """Tickets can transition between any statuses."""
        ticket = await state_manager.create_ticket(Ticket.create(title="Test", status=from_status))
        updated = await state_manager.move_ticket(ticket.id, to_status)
        assert updated is not None and updated.status == to_status

    async def test_full_lifecycle_with_review_data(self, state_manager: StateManager):
        """Complete lifecycle: create → progress → review with summary → done."""
        ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Full lifecycle",
                priority=TicketPriority.HIGH,
                acceptance_criteria=["Handle errors", "Log events"],
            )
        )
        assert ticket.status == TicketStatus.BACKLOG

        for status in [TicketStatus.IN_PROGRESS, TicketStatus.REVIEW]:
            ticket = await state_manager.move_ticket(ticket.id, status)
            assert ticket is not None and ticket.status == status

        ticket = await state_manager.update_ticket(
            ticket.id, status=TicketStatus.DONE, review_summary="All tests pass", checks_passed=True
        )
        assert ticket is not None
        assert ticket.status == TicketStatus.DONE
        assert ticket.review_summary == "All tests pass"
        assert ticket.checks_passed is True
        assert ticket.acceptance_criteria == ["Handle errors", "Log events"]

    async def test_ticket_counts_update_through_lifecycle(self, state_manager: StateManager):
        """Ticket counts update correctly as tickets move through statuses."""
        t1 = await state_manager.create_ticket(Ticket.create(title="T1"))
        await state_manager.create_ticket(Ticket.create(title="T2"))

        counts = await state_manager.get_ticket_counts()
        assert counts[TicketStatus.BACKLOG] == 2 and counts[TicketStatus.IN_PROGRESS] == 0

        for status in [TicketStatus.IN_PROGRESS, TicketStatus.REVIEW, TicketStatus.DONE]:
            await state_manager.move_ticket(t1.id, status)
            counts = await state_manager.get_ticket_counts()
            assert counts[TicketStatus.BACKLOG] == 1 and counts[status] == 1

    async def test_move_nonexistent_ticket_returns_none(self, state_manager: StateManager):
        """Moving a nonexistent ticket returns None."""
        assert await state_manager.move_ticket("nonexistent", TicketStatus.DONE) is None


class TestTicketRejection:
    """Ticket rejection flow: REVIEW → BACKLOG → rework → DONE."""

    async def test_reject_ticket_back_to_backlog(self, state_manager: StateManager):
        """Rejected ticket moves from REVIEW back to BACKLOG preserving fields."""
        ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Feature needs rework",
                priority=TicketPriority.HIGH,
                status=TicketStatus.REVIEW,
            )
        )
        updated = await state_manager.update_ticket(
            ticket.id,
            status=TicketStatus.BACKLOG,
            review_summary="Missing error handling",
            checks_passed=False,
        )
        assert updated is not None
        assert updated.status == TicketStatus.BACKLOG
        assert updated.review_summary == "Missing error handling"
        assert updated.checks_passed is False
        assert updated.title == "Feature needs rework" and updated.priority == TicketPriority.HIGH

    async def test_multiple_rejections_then_approval(self, state_manager: StateManager):
        """Ticket can be rejected multiple times before final approval."""
        ticket = await state_manager.create_ticket(Ticket.create(title="Rework test"))

        for i in range(3):
            await state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)
            await state_manager.move_ticket(ticket.id, TicketStatus.REVIEW)
            ticket = await state_manager.update_ticket(
                ticket.id,
                status=TicketStatus.BACKLOG,
                review_summary=f"Rejection {i + 1}",
                checks_passed=False,
            )
            assert ticket is not None and ticket.status == TicketStatus.BACKLOG

        await state_manager.move_ticket(ticket.id, TicketStatus.IN_PROGRESS)
        await state_manager.move_ticket(ticket.id, TicketStatus.REVIEW)
        ticket = await state_manager.update_ticket(
            ticket.id, status=TicketStatus.DONE, review_summary="Approved!", checks_passed=True
        )
        assert ticket is not None and ticket.status == TicketStatus.DONE


class TestTicketDeletion:
    """Ticket deletion from any status."""

    @pytest.mark.parametrize("status", list(TicketStatus))
    async def test_delete_from_any_status(self, state_manager: StateManager, status: TicketStatus):
        """Tickets can be deleted from any status."""
        ticket = await state_manager.create_ticket(
            Ticket.create(title="Delete test", status=status)
        )
        assert await state_manager.delete_ticket(ticket.id) is True
        assert await state_manager.get_ticket(ticket.id) is None

    async def test_delete_nonexistent_returns_false(self, state_manager: StateManager):
        """Deleting a nonexistent ticket returns False."""
        assert await state_manager.delete_ticket("nonexistent-id") is False

    async def test_delete_and_recreate_same_title(self, state_manager: StateManager):
        """After deletion, can create a new ticket with same title."""
        original = await state_manager.create_ticket(Ticket.create(title="Feature X"))
        original_id = original.id
        await state_manager.delete_ticket(original.id)
        new_ticket = await state_manager.create_ticket(Ticket.create(title="Feature X"))
        assert new_ticket.id != original_id and new_ticket.title == "Feature X"


class TestAutoTicketLifecycle:
    """AUTO ticket lifecycle and scheduler integration."""

    async def test_auto_ticket_maintains_type_through_lifecycle(self, state_manager: StateManager):
        """AUTO ticket maintains type through all status transitions."""
        ticket = await state_manager.create_ticket(
            Ticket.create(title="Auto", ticket_type=TicketType.AUTO)
        )
        assert ticket.ticket_type == TicketType.AUTO

        for status in [TicketStatus.IN_PROGRESS, TicketStatus.REVIEW, TicketStatus.DONE]:
            await state_manager.move_ticket(ticket.id, status)
            ticket = await state_manager.get_ticket(ticket.id)
            assert ticket is not None and ticket.ticket_type == TicketType.AUTO

    async def test_scheduler_ignores_pair_tickets(
        self, scheduler: Scheduler, state_manager: StateManager
    ):
        """Scheduler only processes AUTO tickets, ignoring PAIR tickets."""
        pair_ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Pair", ticket_type=TicketType.PAIR, status=TicketStatus.IN_PROGRESS
            )
        )
        scheduler.start()
        await scheduler.handle_status_change(pair_ticket.id, None, TicketStatus.IN_PROGRESS)
        assert not scheduler.is_running(pair_ticket.id) and len(scheduler._running) == 0

    async def test_scheduler_stop_cleans_up(
        self, scheduler: Scheduler, state_manager: StateManager, mocker
    ):
        """Scheduler.stop() cleans up all running tickets."""
        from kagan.agents.scheduler import RunningTicketState

        mock_agent = mocker.MagicMock()
        mock_agent.stop = mocker.AsyncMock()
        mock_task = mocker.MagicMock()
        mock_task.cancel = mocker.MagicMock()
        mock_task.done = mocker.MagicMock(return_value=False)
        scheduler._running["test-ticket"] = RunningTicketState(
            task=mock_task, agent=mock_agent, iteration=5
        )

        await scheduler.stop()

        assert len(scheduler._running) == 0
        mock_agent.stop.assert_called_once()
        mock_task.cancel.assert_called_once()

    async def test_stop_individual_ticket(
        self, scheduler: Scheduler, state_manager: StateManager, mocker
    ):
        """Individual ticket can be stopped while others continue."""
        import asyncio
        import contextlib

        from kagan.agents.scheduler import RunningTicketState

        async def dummy():
            await asyncio.sleep(100)

        mock_agent1, mock_agent2 = mocker.MagicMock(), mocker.MagicMock()
        mock_agent1.stop, mock_agent2.stop = mocker.AsyncMock(), mocker.AsyncMock()
        task1, task2 = asyncio.create_task(dummy()), asyncio.create_task(dummy())
        task1.cancel()

        scheduler._running["ticket-1"] = RunningTicketState(
            task=task1, agent=mock_agent1, iteration=3
        )
        scheduler._running["ticket-2"] = RunningTicketState(
            task=task2, agent=mock_agent2, iteration=1
        )

        # Start scheduler so worker loop can process stop request
        scheduler.start()
        assert await scheduler.stop_ticket("ticket-1") is True
        # Wait for worker to process the stop event
        await asyncio.sleep(0.2)
        assert "ticket-1" not in scheduler._running and "ticket-2" in scheduler._running
        mock_agent1.stop.assert_called_once()
        mock_agent2.stop.assert_not_called()

        task2.cancel()
        with contextlib.suppress(asyncio.CancelledError):
            await task2
        await scheduler.stop()
