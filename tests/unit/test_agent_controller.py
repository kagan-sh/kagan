"""Unit tests for AgentController.

Tests the agent control operations for AUTO tickets extracted from KanbanScreen.
"""

from __future__ import annotations

from unittest.mock import AsyncMock, MagicMock

import pytest

from kagan.database.models import Ticket, TicketStatus, TicketType
from kagan.ui.screens.kanban.agent_controller import AgentController

pytestmark = pytest.mark.unit


@pytest.fixture
def mock_kagan_app() -> MagicMock:
    """Create a mock KaganApp with scheduler and state_manager."""
    app = MagicMock()
    app.scheduler = MagicMock()
    app.state_manager = AsyncMock()
    app.config = MagicMock()
    app.config.general.auto_start = False
    return app


@pytest.fixture
def mock_callbacks() -> tuple[MagicMock, AsyncMock, AsyncMock]:
    """Create mock callbacks for notify, push_screen, refresh_board."""
    notify = MagicMock()
    push_screen = AsyncMock()
    refresh_board = AsyncMock()
    return notify, push_screen, refresh_board


@pytest.fixture
def agent_controller(
    mock_kagan_app: MagicMock,
    mock_callbacks: tuple[MagicMock, AsyncMock, AsyncMock],
) -> AgentController:
    """Create an AgentController with mocked dependencies."""
    notify, push_screen, refresh_board = mock_callbacks
    return AgentController(
        kagan_app=mock_kagan_app,
        notify=notify,
        push_screen=push_screen,
        refresh_board=refresh_board,
    )


@pytest.fixture
def auto_ticket() -> Ticket:
    """Create an AUTO ticket for testing."""
    return Ticket(
        title="Auto task",
        ticket_type=TicketType.AUTO,
        status=TicketStatus.IN_PROGRESS,
    )


@pytest.fixture
def pair_ticket() -> Ticket:
    """Create a PAIR ticket for testing."""
    return Ticket(
        title="Pair task",
        ticket_type=TicketType.PAIR,
        status=TicketStatus.IN_PROGRESS,
    )


@pytest.fixture
def backlog_auto_ticket() -> Ticket:
    """Create an AUTO ticket in BACKLOG."""
    return Ticket(
        title="Backlog auto task",
        ticket_type=TicketType.AUTO,
        status=TicketStatus.BACKLOG,
    )


# =============================================================================
# watch_agent Tests
# =============================================================================


class TestWatchAgent:
    """Tests for AgentController.watch_agent()."""

    async def test_watch_non_auto_ticket_shows_warning(
        self,
        agent_controller: AgentController,
        pair_ticket: Ticket,
        mock_callbacks: tuple[MagicMock, AsyncMock, AsyncMock],
    ) -> None:
        """Watching a PAIR ticket shows warning."""
        notify, _, _ = mock_callbacks

        await agent_controller.watch_agent(pair_ticket)

        notify.assert_called_once_with("Watch is only for AUTO tickets", "warning")

    async def test_watch_not_running_backlog_shows_start_hint(
        self,
        agent_controller: AgentController,
        backlog_auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, AsyncMock, AsyncMock],
    ) -> None:
        """Watching a BACKLOG ticket that's not running shows start hint."""
        notify, _, _ = mock_callbacks
        mock_kagan_app.scheduler.is_running.return_value = False

        await agent_controller.watch_agent(backlog_auto_ticket)

        notify.assert_called_once()
        assert "Press [a] to start" in notify.call_args[0][0]

    async def test_watch_not_running_in_progress_auto_start_disabled(
        self,
        agent_controller: AgentController,
        auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, AsyncMock, AsyncMock],
    ) -> None:
        """Watching IN_PROGRESS ticket not running with auto_start=False shows manual hint."""
        notify, _, _ = mock_callbacks
        mock_kagan_app.scheduler.is_running.return_value = False
        mock_kagan_app.config.general.auto_start = False

        await agent_controller.watch_agent(auto_ticket)

        notify.assert_called_once()
        assert "Press a to start manually" in notify.call_args[0][0]

    async def test_watch_not_running_in_progress_auto_start_enabled(
        self,
        agent_controller: AgentController,
        auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, AsyncMock, AsyncMock],
    ) -> None:
        """Watching IN_PROGRESS ticket not running with auto_start=True shows tick hint."""
        notify, _, _ = mock_callbacks
        mock_kagan_app.scheduler.is_running.return_value = False
        mock_kagan_app.config.general.auto_start = True

        await agent_controller.watch_agent(auto_ticket)

        notify.assert_called_once()
        assert "next scheduler tick" in notify.call_args[0][0]

    async def test_watch_running_opens_modal(
        self,
        agent_controller: AgentController,
        auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, AsyncMock, AsyncMock],
    ) -> None:
        """Watching a running agent opens the AgentOutputModal."""
        _, push_screen, _ = mock_callbacks
        mock_kagan_app.scheduler.is_running.return_value = True
        mock_kagan_app.scheduler.get_running_agent.return_value = MagicMock()
        mock_kagan_app.scheduler.get_iteration_count.return_value = 1

        await agent_controller.watch_agent(auto_ticket)

        # Modal should be pushed (we can't easily mock the import,
        # but we can verify push_screen was called)
        push_screen.assert_called_once()


# =============================================================================
# start_agent Tests
# =============================================================================


class TestStartAgent:
    """Tests for AgentController.start_agent()."""

    async def test_start_non_auto_ticket_shows_warning(
        self,
        agent_controller: AgentController,
        pair_ticket: Ticket,
        mock_callbacks: tuple[MagicMock, AsyncMock, AsyncMock],
    ) -> None:
        """Starting agent for PAIR ticket shows warning."""
        notify, _, _ = mock_callbacks

        result = await agent_controller.start_agent(pair_ticket)

        assert result is False
        notify.assert_called_once_with("Start agent is only for AUTO tickets", "warning")

    async def test_start_from_backlog_moves_to_in_progress(
        self,
        agent_controller: AgentController,
        backlog_auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, AsyncMock, AsyncMock],
    ) -> None:
        """Starting agent from BACKLOG moves ticket to IN_PROGRESS."""
        _notify, _, refresh_board = mock_callbacks
        mock_kagan_app.scheduler.spawn_for_ticket = AsyncMock(return_value=True)
        mock_kagan_app.state_manager.get_ticket.return_value = backlog_auto_ticket

        result = await agent_controller.start_agent(backlog_auto_ticket)

        assert result is True
        mock_kagan_app.state_manager.move_ticket.assert_called_once_with(
            backlog_auto_ticket.id, TicketStatus.IN_PROGRESS
        )
        refresh_board.assert_called_once()

    async def test_start_success_shows_notification(
        self,
        agent_controller: AgentController,
        auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, AsyncMock, AsyncMock],
    ) -> None:
        """Successful agent start shows notification."""
        notify, _, _ = mock_callbacks
        mock_kagan_app.scheduler.spawn_for_ticket = AsyncMock(return_value=True)

        result = await agent_controller.start_agent(auto_ticket)

        assert result is True
        notify.assert_called_once()
        assert "Started agent for" in notify.call_args[0][0]
        assert notify.call_args[0][1] == "information"

    async def test_start_at_capacity_shows_warning(
        self,
        agent_controller: AgentController,
        auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, AsyncMock, AsyncMock],
    ) -> None:
        """Agent start failure (at capacity) shows warning."""
        notify, _, _ = mock_callbacks
        mock_kagan_app.scheduler.spawn_for_ticket = AsyncMock(return_value=False)

        result = await agent_controller.start_agent(auto_ticket)

        assert result is False
        notify.assert_called_once()
        assert "at capacity" in notify.call_args[0][0]


# =============================================================================
# stop_agent Tests
# =============================================================================


class TestStopAgent:
    """Tests for AgentController.stop_agent()."""

    async def test_stop_non_auto_ticket_shows_warning(
        self,
        agent_controller: AgentController,
        pair_ticket: Ticket,
        mock_callbacks: tuple[MagicMock, AsyncMock, AsyncMock],
    ) -> None:
        """Stopping agent for PAIR ticket shows warning."""
        notify, _, _ = mock_callbacks

        result = await agent_controller.stop_agent(pair_ticket)

        assert result is False
        notify.assert_called_once_with("Stop agent is only for AUTO tickets", "warning")

    async def test_stop_not_running_shows_warning(
        self,
        agent_controller: AgentController,
        auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, AsyncMock, AsyncMock],
    ) -> None:
        """Stopping a non-running agent shows warning."""
        notify, _, _ = mock_callbacks
        mock_kagan_app.scheduler.is_running.return_value = False

        result = await agent_controller.stop_agent(auto_ticket)

        assert result is False
        notify.assert_called_once_with("No agent running for this ticket", "warning")

    async def test_stop_success_moves_to_backlog(
        self,
        agent_controller: AgentController,
        auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, AsyncMock, AsyncMock],
    ) -> None:
        """Successful agent stop moves ticket to BACKLOG."""
        notify, _, refresh_board = mock_callbacks
        mock_kagan_app.scheduler.is_running.return_value = True
        mock_kagan_app.scheduler.stop_ticket = AsyncMock()

        result = await agent_controller.stop_agent(auto_ticket)

        assert result is True
        mock_kagan_app.scheduler.stop_ticket.assert_called_once_with(auto_ticket.id)
        mock_kagan_app.state_manager.move_ticket.assert_called_once_with(
            auto_ticket.id, TicketStatus.BACKLOG
        )
        refresh_board.assert_called_once()
        notify.assert_called_once()
        assert "Stopped agent" in notify.call_args[0][0]
        assert "BACKLOG" in notify.call_args[0][0]
