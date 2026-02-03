"""Snapshot tests for Kanban screen user flows.

These tests cover the main Kanban interaction flows:
- Board display with tickets in columns
- Column and ticket navigation
- Leader mode activation
- Search functionality
- Modals (create, view, delete, peek)

Note: Tests are synchronous because pytest-textual-snapshot's snap_compare
internally calls asyncio.run(), which conflicts with async test functions.
"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Any

import pytest

from kagan.app import KaganApp
from kagan.database.manager import StateManager
from kagan.database.models import Ticket, TicketPriority, TicketStatus, TicketType

if TYPE_CHECKING:
    from types import SimpleNamespace

    from textual.pilot import Pilot

    from tests.snapshots.conftest import MockAgentFactory


def _create_fake_tmux(sessions: dict[str, Any]) -> object:
    """Create a fake tmux function that tracks session state."""

    async def fake_run_tmux(*args: str) -> str:
        if not args:
            return ""
        command, args_list = args[0], list(args)
        if command == "new-session" and "-s" in args_list:
            idx = args_list.index("-s")
            name = args_list[idx + 1] if idx + 1 < len(args_list) else None
            if name:
                cwd = args_list[args_list.index("-c") + 1] if "-c" in args_list else ""
                env: dict[str, str] = {}
                for i, val in enumerate(args_list):
                    if val == "-e" and i + 1 < len(args_list):
                        key, _, env_value = args_list[i + 1].partition("=")
                        env[key] = env_value
                sessions[name] = {"cwd": cwd, "env": env, "sent_keys": []}
        elif command == "kill-session" and "-t" in args_list:
            sessions.pop(args_list[args_list.index("-t") + 1], None)
        elif command == "send-keys" and "-t" in args_list:
            idx = args_list.index("-t")
            name = args_list[idx + 1]
            keys = args_list[idx + 2] if idx + 2 < len(args_list) else ""
            if name in sessions:
                sessions[name]["sent_keys"].append(keys)
        elif command == "list-sessions":
            return "\n".join(sorted(sessions.keys()))
        return ""

    return fake_run_tmux


async def _setup_kanban_tickets(db_path: str) -> None:
    """Pre-populate database with tickets in different columns.

    Uses fixed IDs for snapshot reproducibility.
    """
    manager = StateManager(db_path)
    await manager.initialize()

    # Create tickets with fixed IDs for reproducible snapshots
    tickets = [
        Ticket(
            id="backlog1",
            title="Backlog task 1",
            description="First task in backlog",
            priority=TicketPriority.LOW,
            status=TicketStatus.BACKLOG,
            ticket_type=TicketType.PAIR,
        ),
        Ticket(
            id="backlog2",
            title="Backlog task 2",
            description="Second task in backlog",
            priority=TicketPriority.HIGH,
            status=TicketStatus.BACKLOG,
            ticket_type=TicketType.AUTO,
        ),
        Ticket(
            id="inprog01",
            title="In progress task",
            description="Currently working on this",
            priority=TicketPriority.HIGH,
            status=TicketStatus.IN_PROGRESS,
            ticket_type=TicketType.PAIR,
        ),
        Ticket(
            id="review01",
            title="Review task",
            description="Ready for code review",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
            ticket_type=TicketType.AUTO,
        ),
        Ticket(
            id="done0001",
            title="Done task",
            description="Completed work",
            priority=TicketPriority.LOW,
            status=TicketStatus.DONE,
            ticket_type=TicketType.PAIR,
        ),
    ]

    for ticket in tickets:
        await manager.create_ticket(ticket)
    await manager.close()


class TestKanbanFlow:
    """Snapshot tests for Kanban screen user flows."""

    @pytest.fixture
    def kanban_app(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        monkeypatch: pytest.MonkeyPatch,
    ) -> KaganApp:
        """Create app with pre-populated tickets for kanban testing."""
        # Mock tmux
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", fake_tmux)

        # Set up tickets synchronously
        loop = asyncio.new_event_loop()
        try:
            loop.run_until_complete(_setup_kanban_tickets(snapshot_project.db))
        finally:
            loop.close()

        return KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            agent_factory=mock_acp_agent_factory,
        )

    @pytest.mark.snapshot
    def test_kanban_board_with_tickets(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Kanban board displays tickets in their respective columns."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Verify we're on KanbanScreen (since we have tickets)
            from kagan.ui.screens.kanban import KanbanScreen

            assert isinstance(pilot.app.screen, KanbanScreen)

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_kanban_navigate_columns(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """User navigates between columns with right arrow key."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate right twice to reach REVIEW column
            await pilot.press("right")
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_kanban_navigate_tickets(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """User navigates tickets within a column using up/down arrows."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate down within backlog column (has 2 tickets)
            await pilot.press("down")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_kanban_ticket_focused(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Focused ticket has visual highlight indicator."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Focus should be on first ticket by default
            from kagan.ui.screens.kanban import KanbanScreen

            screen = pilot.app.screen
            assert isinstance(screen, KanbanScreen)

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_kanban_leader_mode(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing 'g' activates leader mode and shows leader hint bar."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Activate leader mode
            await pilot.press("g")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_kanban_search_bar_open(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing '/' opens the search bar."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open search bar
            await pilot.press("slash")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_kanban_search_with_query(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """User types search query in search bar, tickets are filtered."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open search bar and type query
            await pilot.press("slash")
            await pilot.pause()
            # Type "backlog" to filter
            for char in "backlog":
                await pilot.press(char)
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_kanban_create_ticket_modal(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing 'n' opens the new ticket modal."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open create ticket modal
            await pilot.press("n")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_kanban_create_auto_ticket_modal(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing 'N' (Shift+N) opens new ticket modal with AUTO type preselected."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open create AUTO ticket modal
            await pilot.press("N")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_kanban_view_ticket_details(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing 'v' opens ticket details modal for the focused ticket."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # View ticket details
            await pilot.press("v")
            # Multiple pauses to ensure modal is fully mounted
            await pilot.pause()
            await pilot.pause()
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_kanban_peek_overlay(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing 'space' shows peek overlay with ticket scratchpad preview."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Toggle peek overlay
            await pilot.press("space")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_kanban_delete_confirmation(
        self,
        kanban_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Pressing 'x' shows delete confirmation modal."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Open delete confirmation
            await pilot.press("x")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(kanban_app, terminal_size=(cols, rows), run_before=run_before)
