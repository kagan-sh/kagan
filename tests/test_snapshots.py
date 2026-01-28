"""Snapshot tests for Kagan TUI.

The empty board test is fully deterministic. Tests with tickets contain
dynamic content (random IDs, current dates) and are skipped by default.
Run with -m 'not skip_by_default' or manually use --snapshot-update.
"""

import os

import pytest

from kagan.app import KaganApp
from kagan.database.models import TicketCreate, TicketPriority, TicketStatus
from kagan.ui.screens.kanban import KanbanScreen


def test_empty_board(snap_compare):
    """Empty kanban board with 4 columns."""

    async def setup(pilot):
        # Push KanbanScreen explicitly (empty board would normally show PlannerScreen)
        await pilot.app.push_screen(KanbanScreen())
        await pilot.pause()

    assert snap_compare(
        app=KaganApp(db_path=":memory:"),
        terminal_size=(100, 30),
        run_before=setup,
    )


@pytest.mark.skipif(
    os.environ.get("UPDATE_SNAPSHOTS") != "1",
    reason="Contains dynamic content - set UPDATE_SNAPSHOTS=1 to run",
)
def test_board_with_tickets(snap_compare):
    """Board with tickets in multiple columns."""

    async def setup(pilot):
        sm = pilot.app.state_manager
        await sm.create_ticket(
            TicketCreate(
                title="Backlog task",
                description="A task in backlog",
                priority=TicketPriority.LOW,
                status=TicketStatus.BACKLOG,
            )
        )
        await sm.create_ticket(
            TicketCreate(
                title="In progress task",
                description="Currently working",
                priority=TicketPriority.HIGH,
                status=TicketStatus.IN_PROGRESS,
            )
        )
        await sm.create_ticket(
            TicketCreate(
                title="Done task",
                description="Completed work",
                priority=TicketPriority.MEDIUM,
                status=TicketStatus.DONE,
            )
        )
        screen = pilot.app.screen
        if isinstance(screen, KanbanScreen):
            await screen._refresh_board()
        await pilot.pause()

    assert snap_compare(
        app=KaganApp(db_path=":memory:"),
        terminal_size=(100, 30),
        run_before=setup,
    )


@pytest.mark.skipif(
    os.environ.get("UPDATE_SNAPSHOTS") != "1",
    reason="Contains dynamic content - set UPDATE_SNAPSHOTS=1 to run",
)
def test_ticket_priorities(snap_compare):
    """All 3 priority levels displayed."""

    async def setup(pilot):
        sm = pilot.app.state_manager
        await sm.create_ticket(TicketCreate(title="High priority", priority=TicketPriority.HIGH))
        await sm.create_ticket(
            TicketCreate(title="Medium priority", priority=TicketPriority.MEDIUM)
        )
        await sm.create_ticket(TicketCreate(title="Low priority", priority=TicketPriority.LOW))
        screen = pilot.app.screen
        if isinstance(screen, KanbanScreen):
            await screen._refresh_board()
        await pilot.pause()

    assert snap_compare(
        app=KaganApp(db_path=":memory:"),
        terminal_size=(100, 30),
        run_before=setup,
    )
