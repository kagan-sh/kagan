"""Snapshot tests for ticket movement between columns.

These tests specifically verify that tickets don't appear in multiple columns
when moved using leader keys (gl/gh) or other navigation.
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


async def _setup_movement_tickets(db_path: str) -> None:
    """Create tickets for movement testing."""
    manager = StateManager(db_path)
    await manager.initialize()

    # Create PAIR tickets in different columns
    tickets = [
        Ticket(
            id="pair0001",
            title="PAIR in Backlog",
            description="PAIR ticket ready to move",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.BACKLOG,
            ticket_type=TicketType.PAIR,
        ),
        Ticket(
            id="pair0002",
            title="PAIR in Progress",
            description="PAIR ticket in progress",
            priority=TicketPriority.HIGH,
            status=TicketStatus.IN_PROGRESS,
            ticket_type=TicketType.PAIR,
        ),
    ]

    for ticket in tickets:
        await manager.create_ticket(ticket)
    await manager.close()


async def _setup_auto_movement_ticket(db_path: str) -> None:
    """Create an AUTO ticket in progress for movement confirmation testing."""
    manager = StateManager(db_path)
    await manager.initialize()

    ticket = Ticket(
        id="auto0001",
        title="AUTO in Progress",
        description="AUTO ticket in progress",
        priority=TicketPriority.HIGH,
        status=TicketStatus.IN_PROGRESS,
        ticket_type=TicketType.AUTO,
    )

    await manager.create_ticket(ticket)
    await manager.close()


class TestTicketMovement:
    """Snapshot tests for ticket movement to detect multi-column bugs."""

    @pytest.fixture
    def movement_app(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        monkeypatch: pytest.MonkeyPatch,
    ) -> KaganApp:
        """Create app with PAIR tickets for movement testing."""
        # Mock tmux
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", fake_tmux)

        # Set up tickets synchronously
        loop = asyncio.new_event_loop()
        try:
            loop.run_until_complete(_setup_movement_tickets(snapshot_project.db))
        finally:
            loop.close()

        return KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            agent_factory=mock_acp_agent_factory,
        )

    @pytest.fixture
    def auto_movement_app(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        monkeypatch: pytest.MonkeyPatch,
    ) -> KaganApp:
        """Create app with AUTO ticket for confirmation modal testing."""
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", fake_tmux)

        loop = asyncio.new_event_loop()
        try:
            loop.run_until_complete(_setup_auto_movement_ticket(snapshot_project.db))
        finally:
            loop.close()

        return KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            agent_factory=mock_acp_agent_factory,
        )

    @pytest.mark.snapshot
    def test_pair_ticket_move_forward_backlog_to_progress(
        self,
        movement_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Move PAIR ticket forward from BACKLOG to IN_PROGRESS using gl."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Focus on first ticket in BACKLOG
            from kagan.ui.screens.kanban import KanbanScreen

            assert isinstance(pilot.app.screen, KanbanScreen)

            # Press g then l to move forward
            await pilot.press("g")
            await pilot.pause()
            await pilot.press("l")
            await pilot.pause()

            # Wait for board to refresh and UI to synchronize
            await asyncio.sleep(0.3)
            await pilot.pause()
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(movement_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_pair_ticket_move_forward_progress_to_review(
        self,
        movement_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Move PAIR ticket forward from IN_PROGRESS to REVIEW using gl with confirmation."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate to IN_PROGRESS column
            await pilot.press("right")
            await pilot.pause()

            # Press g then l to move forward (will show confirmation modal)
            await pilot.press("g")
            await pilot.pause()
            await pilot.press("l")
            await pilot.pause()

            # Modal should appear - press 'y' to confirm
            await pilot.press("y")
            await pilot.pause()

            # Wait for board to refresh and UI to synchronize
            await asyncio.sleep(0.3)
            await pilot.pause()
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(movement_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_pair_ticket_move_backward_progress_to_backlog(
        self,
        movement_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Move PAIR ticket backward from IN_PROGRESS to BACKLOG using gh."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Navigate to IN_PROGRESS column (has pair0002)
            await pilot.press("right")
            await pilot.pause()

            # Press g then h to move backward
            await pilot.press("g")
            await pilot.pause()
            await pilot.press("h")
            await pilot.pause()

            # Wait for board to refresh and UI to synchronize
            await asyncio.sleep(0.3)
            await pilot.pause()
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(movement_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_board_state_after_movement(
        self,
        movement_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Verify board shows correct column counts after ticket movement."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()

            # Initial state: BACKLOG(1) IN_PROGRESS(1)
            # Move first ticket forward: BACKLOG(0) IN_PROGRESS(2)
            await pilot.press("g")
            await pilot.pause()
            await pilot.press("l")
            await pilot.pause()

            # Wait for full synchronization
            await asyncio.sleep(0.5)
            await pilot.pause()
            await pilot.pause()
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(movement_app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_auto_ticket_move_shows_confirm_modal(
        self,
        auto_movement_app: KaganApp,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """AUTO ticket move shows confirmation modal to stop agent and move."""

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Attempt to move AUTO ticket forward (g + l)
            await pilot.press("g")
            await pilot.pause()
            await pilot.press("l")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(auto_movement_app, terminal_size=(cols, rows), run_before=run_before)
