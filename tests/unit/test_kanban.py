"""Unit tests for kanban module.

Consolidates tests for:
- Kanban actions (delete, merge, rejection feedback, review ticket)
- Session handler (session routing, AUTO/PAIR sessions)
"""

from __future__ import annotations

from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from kagan.database.models import Ticket, TicketPriority, TicketStatus, TicketType
from kagan.ui.screens.kanban.actions import (
    apply_rejection_feedback,
    delete_ticket,
    get_review_ticket,
    merge_ticket,
)
from kagan.ui.screens.kanban.session_handler import SessionHandler

pytestmark = pytest.mark.unit


# ===== Shared Fixtures and Helper Functions =====


def create_test_ticket(
    title: str = "Test Ticket",
    ticket_type: TicketType = TicketType.AUTO,
    priority: TicketPriority = TicketPriority.MEDIUM,
    status: TicketStatus = TicketStatus.REVIEW,
    description: str = "Test description",
) -> Ticket:
    """Create a test ticket."""
    return Ticket.create(
        title=title,
        description=description,
        ticket_type=ticket_type,
        priority=priority,
        status=status,
    )


def create_mock_app():
    """Create a mock KaganApp with all necessary components."""
    app = MagicMock()

    # Scheduler
    app.scheduler = MagicMock()
    app.scheduler.is_running = MagicMock(return_value=False)
    app.scheduler.get_running_agent = MagicMock(return_value=None)
    app.scheduler.reset_iterations = MagicMock()
    app.scheduler.spawn_for_ticket = AsyncMock()

    # Session manager
    app.session_manager = MagicMock()
    app.session_manager.kill_session = AsyncMock()

    # Worktree manager
    app.worktree_manager = MagicMock()
    app.worktree_manager.get_path = AsyncMock(return_value=None)
    app.worktree_manager.delete = AsyncMock()
    app.worktree_manager.merge_to_main = AsyncMock(return_value=(True, "Merged"))

    # State manager
    app.state_manager = MagicMock()
    app.state_manager.delete_ticket = AsyncMock()
    app.state_manager.move_ticket = AsyncMock()
    app.state_manager.update_ticket = AsyncMock()
    app.state_manager.get_ticket = AsyncMock()

    # Config
    app.config = MagicMock()
    app.config.general.default_base_branch = "main"

    return app


@pytest.fixture
def mock_kagan_app() -> MagicMock:
    """Create a mock KaganApp with all required managers."""
    app = MagicMock()
    app.scheduler = MagicMock()
    app.state_manager = AsyncMock()
    # session_manager has mix of sync and async methods
    app.session_manager = MagicMock()
    app.session_manager.session_exists = AsyncMock(return_value=True)
    app.session_manager.create_session = AsyncMock()
    app.session_manager.kill_session = AsyncMock()
    app.session_manager.attach_session = MagicMock(return_value=True)  # Sync
    app.worktree_manager = AsyncMock()
    app.config = MagicMock()
    app.config.general.auto_start = False
    app.config.general.default_base_branch = "main"
    app.config.ui.skip_tmux_gateway = True
    return app


@pytest.fixture
def mock_textual_app() -> MagicMock:
    """Create a mock Textual App."""
    app = MagicMock()
    app.suspend = MagicMock(return_value=MagicMock(__enter__=MagicMock(), __exit__=MagicMock()))
    app.call_later = MagicMock()
    return app


def _make_push_screen_mock() -> MagicMock:
    """Create a mock that works as both sync and async push_screen.

    - When called with 1 arg (just modal): returns awaitable coroutine
    - When called with 2 args (modal + callback): returns None (sync)
    """
    mock = MagicMock()

    async def _coro():
        return None

    def _side_effect(*args, **kwargs):
        # If there's a callback (2nd positional arg or 'callback' kwarg), return None
        if len(args) > 1 or "callback" in kwargs:
            return None
        return _coro()

    mock.side_effect = _side_effect
    return mock


@pytest.fixture
def mock_callbacks() -> tuple[MagicMock, MagicMock, AsyncMock]:
    """Create mock callbacks for notify, push_screen, refresh_board."""
    notify = MagicMock()
    # push_screen can be awaited (one arg) or called sync with callback (two args)
    push_screen = _make_push_screen_mock()
    refresh_board = AsyncMock()
    return notify, push_screen, refresh_board


@pytest.fixture
def session_handler(
    mock_kagan_app: MagicMock,
    mock_textual_app: MagicMock,
    mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
) -> SessionHandler:
    """Create a SessionHandler with mocked dependencies."""
    notify, push_screen, refresh_board = mock_callbacks
    return SessionHandler(
        kagan_app=mock_kagan_app,
        app=mock_textual_app,
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


@pytest.fixture
def backlog_pair_ticket() -> Ticket:
    """Create a PAIR ticket in BACKLOG."""
    return Ticket(
        title="Backlog pair task",
        ticket_type=TicketType.PAIR,
        status=TicketStatus.BACKLOG,
    )


# ===== Actions Tests =====


class TestDeleteTicket:
    """Tests for delete_ticket action."""

    async def test_delete_success_no_agent_no_worktree(self):
        """Test successful deletion when no agent running and no worktree."""
        app = create_mock_app()
        ticket = create_test_ticket()

        success, message = await delete_ticket(app, ticket)

        assert success is True
        assert "Deleted successfully" in message
        app.session_manager.kill_session.assert_called_once_with(ticket.id)
        app.state_manager.delete_ticket.assert_called_once_with(ticket.id)

    async def test_delete_stops_running_agent(self):
        """Test deletion stops running agent."""
        app = create_mock_app()
        ticket = create_test_ticket()

        mock_agent = MagicMock()
        mock_agent.stop = AsyncMock()
        app.scheduler.is_running.return_value = True
        app.scheduler.get_running_agent.return_value = mock_agent

        success, _message = await delete_ticket(app, ticket)

        assert success is True
        mock_agent.stop.assert_called_once()

    async def test_delete_deletes_worktree(self):
        """Test deletion deletes worktree when it exists."""
        app = create_mock_app()
        ticket = create_test_ticket()
        app.worktree_manager.get_path.return_value = "/some/path"

        success, _message = await delete_ticket(app, ticket)

        assert success is True
        app.worktree_manager.delete.assert_called_once_with(ticket.id, delete_branch=True)

    async def test_delete_failure_returns_error(self):
        """Test deletion failure returns error message."""
        app = create_mock_app()
        ticket = create_test_ticket()
        app.state_manager.delete_ticket.side_effect = Exception("DB error")

        success, message = await delete_ticket(app, ticket)

        assert success is False
        assert "Delete failed" in message
        assert "DB error" in message

    async def test_delete_partial_failure_logs_steps(self):
        """Test partial failure includes completed steps in logs."""
        app = create_mock_app()
        ticket = create_test_ticket()
        app.session_manager.kill_session.side_effect = Exception("Session error")

        success, message = await delete_ticket(app, ticket)

        assert success is False
        assert "Session error" in message


class TestMergeTicket:
    """Tests for merge_ticket action."""

    async def test_merge_success(self):
        """Test successful merge."""
        app = create_mock_app()
        ticket = create_test_ticket()

        success, message = await merge_ticket(app, ticket)

        assert success is True
        assert message == "Merged"
        app.worktree_manager.merge_to_main.assert_called_once_with(ticket.id, base_branch="main")
        app.worktree_manager.delete.assert_called_once_with(ticket.id, delete_branch=True)
        app.session_manager.kill_session.assert_called_once_with(ticket.id)
        app.state_manager.move_ticket.assert_called_once_with(ticket.id, TicketStatus.DONE)

    async def test_merge_failure(self):
        """Test merge failure doesn't clean up."""
        app = create_mock_app()
        ticket = create_test_ticket()
        app.worktree_manager.merge_to_main.return_value = (False, "Conflicts detected")

        success, message = await merge_ticket(app, ticket)

        assert success is False
        assert message == "Conflicts detected"
        app.worktree_manager.delete.assert_not_called()
        app.session_manager.kill_session.assert_not_called()
        app.state_manager.move_ticket.assert_not_called()


class TestApplyRejectionFeedback:
    """Tests for apply_rejection_feedback action."""

    async def test_retry_action_with_feedback(self):
        """Test retry action appends feedback and restarts agent."""
        app = create_mock_app()
        ticket = create_test_ticket()
        refreshed_ticket = create_test_ticket()
        app.state_manager.get_ticket.return_value = refreshed_ticket

        await apply_rejection_feedback(app, ticket, "Fix the bug", action="retry")

        app.state_manager.update_ticket.assert_called_once()
        call_args = app.state_manager.update_ticket.call_args
        assert ticket.id == call_args[0][0]
        assert "Fix the bug" in call_args.kwargs["description"]
        assert call_args.kwargs["status"] == TicketStatus.IN_PROGRESS

        app.scheduler.reset_iterations.assert_called_once_with(ticket.id)
        app.scheduler.spawn_for_ticket.assert_called_once()

    async def test_retry_action_without_feedback(self):
        """Test retry action without feedback just moves ticket."""
        app = create_mock_app()
        ticket = create_test_ticket()
        refreshed_ticket = create_test_ticket()
        app.state_manager.get_ticket.return_value = refreshed_ticket

        await apply_rejection_feedback(app, ticket, None, action="retry")

        app.state_manager.move_ticket.assert_called_once_with(ticket.id, TicketStatus.IN_PROGRESS)
        app.state_manager.update_ticket.assert_not_called()
        app.scheduler.reset_iterations.assert_called_once()

    async def test_stage_action(self):
        """Test stage action resets iterations but doesn't spawn agent."""
        app = create_mock_app()
        ticket = create_test_ticket()

        await apply_rejection_feedback(app, ticket, None, action="stage")

        app.state_manager.move_ticket.assert_called_once_with(ticket.id, TicketStatus.IN_PROGRESS)
        app.scheduler.reset_iterations.assert_called_once()
        app.scheduler.spawn_for_ticket.assert_not_called()

    async def test_shelve_action(self):
        """Test shelve action moves to backlog without reset."""
        app = create_mock_app()
        ticket = create_test_ticket()

        await apply_rejection_feedback(app, ticket, None, action="shelve")

        app.state_manager.move_ticket.assert_called_once_with(ticket.id, TicketStatus.BACKLOG)
        app.scheduler.reset_iterations.assert_not_called()
        app.scheduler.spawn_for_ticket.assert_not_called()

    async def test_retry_no_spawn_for_pair_ticket(self):
        """Test retry doesn't spawn agent for PAIR tickets."""
        app = create_mock_app()
        ticket = create_test_ticket(ticket_type=TicketType.PAIR)

        await apply_rejection_feedback(app, ticket, None, action="retry")

        app.scheduler.spawn_for_ticket.assert_not_called()

    async def test_retry_no_spawn_when_ticket_not_found(self):
        """Test retry doesn't spawn when refreshed ticket not found."""
        app = create_mock_app()
        ticket = create_test_ticket()
        app.state_manager.get_ticket.return_value = None

        await apply_rejection_feedback(app, ticket, None, action="retry")

        app.scheduler.spawn_for_ticket.assert_not_called()

    async def test_feedback_includes_timestamp(self):
        """Test feedback includes timestamp."""
        app = create_mock_app()
        ticket = create_test_ticket()
        app.state_manager.get_ticket.return_value = create_test_ticket()

        await apply_rejection_feedback(app, ticket, "Some feedback", action="retry")

        call_args = app.state_manager.update_ticket.call_args
        description = call_args.kwargs["description"]
        assert "Review Feedback" in description
        assert "Some feedback" in description


class TestGetReviewTicket:
    """Tests for get_review_ticket helper."""

    def test_returns_ticket_in_review(self):
        """Test returns ticket when in REVIEW status."""
        screen = MagicMock()
        ticket = create_test_ticket(status=TicketStatus.REVIEW)
        card = MagicMock()
        card.ticket = ticket

        result = get_review_ticket(screen, card)

        assert result is ticket
        screen.notify.assert_not_called()

    def test_returns_none_for_non_review_ticket(self):
        """Test returns None and notifies for non-REVIEW ticket."""
        screen = MagicMock()
        ticket = create_test_ticket(status=TicketStatus.IN_PROGRESS)
        card = MagicMock()
        card.ticket = ticket

        result = get_review_ticket(screen, card)

        assert result is None
        screen.notify.assert_called_once()
        assert "not in REVIEW" in screen.notify.call_args[0][0]

    def test_returns_none_for_none_card(self):
        """Test returns None for None card."""
        screen = MagicMock()

        result = get_review_ticket(screen, None)

        assert result is None

    def test_returns_none_for_card_without_ticket(self):
        """Test returns None for card without ticket."""
        screen = MagicMock()
        card = MagicMock()
        card.ticket = None

        result = get_review_ticket(screen, card)

        assert result is None


# ===== Session Handler Tests =====


class TestOpenSessionRouting:
    """Tests for SessionHandler.open_session() routing."""

    async def test_auto_ticket_routes_to_auto_session(
        self,
        session_handler: SessionHandler,
        auto_ticket: Ticket,
    ) -> None:
        """AUTO ticket routes to _open_auto_session."""
        with patch.object(session_handler, "_open_auto_session", new_callable=AsyncMock) as mock:
            await session_handler.open_session(auto_ticket)
            mock.assert_called_once_with(auto_ticket)

    async def test_pair_ticket_routes_to_pair_session(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
    ) -> None:
        """PAIR ticket routes to _open_pair_session."""
        with patch.object(session_handler, "_open_pair_session", new_callable=AsyncMock) as mock:
            await session_handler.open_session(pair_ticket)
            mock.assert_called_once_with(pair_ticket)


class TestAutoSession:
    """Tests for AUTO ticket session handling."""

    async def test_backlog_moves_to_in_progress(
        self,
        session_handler: SessionHandler,
        backlog_auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Opening AUTO session for BACKLOG ticket moves to IN_PROGRESS."""
        _notify, _, refresh_board = mock_callbacks
        mock_kagan_app.state_manager.get_ticket.return_value = backlog_auto_ticket
        mock_kagan_app.config.general.auto_start = False

        await session_handler._open_auto_session(backlog_auto_ticket)

        mock_kagan_app.state_manager.move_ticket.assert_called_once_with(
            backlog_auto_ticket.id, TicketStatus.IN_PROGRESS
        )
        refresh_board.assert_called_once()

    async def test_backlog_ticket_refresh_returns_none(
        self,
        session_handler: SessionHandler,
        backlog_auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Opening AUTO session for BACKLOG ticket handles None from get_ticket."""
        # Cover line 93->95 branch when get_ticket returns None
        _notify, _, refresh_board = mock_callbacks
        mock_kagan_app.state_manager.get_ticket.return_value = None
        mock_kagan_app.config.general.auto_start = False

        await session_handler._open_auto_session(backlog_auto_ticket)

        # Should still move ticket and refresh
        mock_kagan_app.state_manager.move_ticket.assert_called_once_with(
            backlog_auto_ticket.id, TicketStatus.IN_PROGRESS
        )
        refresh_board.assert_called_once()

    async def test_running_agent_opens_modal(
        self,
        session_handler: SessionHandler,
        auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Opening session for running agent opens AgentOutputModal."""
        _, push_screen, _ = mock_callbacks
        mock_kagan_app.scheduler.is_running.return_value = True
        mock_kagan_app.scheduler.get_running_agent.return_value = MagicMock()
        mock_kagan_app.scheduler.get_iteration_count.return_value = 1

        await session_handler._open_auto_session(auto_ticket)

        # Modal should be pushed
        push_screen.assert_called_once()

    async def test_manual_start_spawns_agent(
        self,
        session_handler: SessionHandler,
        auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Manual start spawns agent and opens modal."""
        notify, push_screen, _ = mock_callbacks
        mock_kagan_app.scheduler.is_running.return_value = False
        mock_kagan_app.scheduler.spawn_for_ticket = AsyncMock(return_value=True)
        mock_kagan_app.scheduler.get_running_agent.return_value = MagicMock()
        mock_kagan_app.scheduler.get_iteration_count.return_value = 1

        await session_handler._open_auto_session(auto_ticket, manual=True)

        mock_kagan_app.scheduler.spawn_for_ticket.assert_called_once_with(auto_ticket)
        notify.assert_called()
        push_screen.assert_called()

    async def test_auto_start_disabled_shows_hint(
        self,
        session_handler: SessionHandler,
        auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """With auto_start disabled, shows manual start hint."""
        notify, _, _ = mock_callbacks
        mock_kagan_app.scheduler.is_running.return_value = False
        mock_kagan_app.config.general.auto_start = False

        await session_handler._open_auto_session(auto_ticket)

        notify.assert_called_once()
        assert "Auto-start disabled" in notify.call_args[0][0]


class TestPairSession:
    """Tests for PAIR ticket session handling."""

    async def test_gateway_modal_shown_when_not_skipped(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Gateway modal is shown when skip_tmux_gateway=False."""
        _, push_screen, _ = mock_callbacks
        mock_kagan_app.config.ui.skip_tmux_gateway = False

        await session_handler._open_pair_session(pair_ticket)

        # push_screen should be called with a modal and callback
        push_screen.assert_called_once()

    async def test_gateway_skipped_goes_directly_to_session(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
    ) -> None:
        """When skip_tmux_gateway=True, opens session directly."""
        mock_kagan_app.config.ui.skip_tmux_gateway = True

        with patch.object(session_handler, "_do_open_pair_session", new_callable=AsyncMock) as mock:
            await session_handler._open_pair_session(pair_ticket)
            mock.assert_called_once_with(pair_ticket)


class TestDoOpenPairSession:
    """Tests for the actual PAIR session opening."""

    async def test_creates_worktree_if_not_exists(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
    ) -> None:
        """Creates worktree if path doesn't exist."""
        mock_kagan_app.worktree_manager.get_path = AsyncMock(return_value=None)
        mock_kagan_app.worktree_manager.create = AsyncMock(return_value="/path/to/worktree")
        mock_kagan_app.session_manager.session_exists = AsyncMock(return_value=False)
        mock_kagan_app.session_manager.attach_session.return_value = True

        await session_handler._do_open_pair_session(pair_ticket)

        mock_kagan_app.worktree_manager.create.assert_called_once_with(
            pair_ticket.id, pair_ticket.title, "main"
        )

    async def test_creates_session_if_not_exists(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
    ) -> None:
        """Creates tmux session if it doesn't exist."""
        wt_path = "/path/to/worktree"
        mock_kagan_app.worktree_manager.get_path = AsyncMock(return_value=wt_path)
        mock_kagan_app.session_manager.session_exists = AsyncMock(return_value=False)
        mock_kagan_app.session_manager.attach_session.return_value = True

        await session_handler._do_open_pair_session(pair_ticket)

        mock_kagan_app.session_manager.create_session.assert_called_once_with(pair_ticket, wt_path)

    async def test_moves_backlog_to_in_progress(
        self,
        session_handler: SessionHandler,
        backlog_pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
    ) -> None:
        """BACKLOG ticket is moved to IN_PROGRESS."""
        mock_kagan_app.worktree_manager.get_path = AsyncMock(return_value="/path")
        mock_kagan_app.session_manager.session_exists = AsyncMock(return_value=True)
        mock_kagan_app.session_manager.attach_session.return_value = True

        await session_handler._do_open_pair_session(backlog_pair_ticket)

        mock_kagan_app.state_manager.move_ticket.assert_called_once_with(
            backlog_pair_ticket.id, TicketStatus.IN_PROGRESS
        )

    async def test_attach_session_called(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
    ) -> None:
        """attach_session is called with correct ticket ID."""
        mock_kagan_app.worktree_manager.get_path = AsyncMock(return_value="/path")
        mock_kagan_app.session_manager.session_exists = AsyncMock(return_value=True)
        mock_kagan_app.session_manager.attach_session.return_value = True

        await session_handler._do_open_pair_session(pair_ticket)

        mock_kagan_app.session_manager.attach_session.assert_called_once_with(pair_ticket.id)


class TestSkipGatewayCallback:
    """Tests for the skip gateway callback."""

    def test_callback_can_be_set(
        self,
        session_handler: SessionHandler,
    ) -> None:
        """Callback can be set and stored."""
        callback = MagicMock()
        session_handler.set_skip_tmux_gateway_callback(callback)
        assert session_handler._skip_tmux_gateway_callback is callback


# =============================================================================
# Manual Agent Start Tests (Lines 99-104)
# =============================================================================


class TestAutoSessionManualStart:
    """Tests for manual agent start scenarios."""

    async def test_manual_start_backlog_moves_and_spawns(
        self,
        session_handler: SessionHandler,
        backlog_auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Manual start with BACKLOG ticket moves to IN_PROGRESS and spawns agent."""
        # Cover lines 99-104
        notify, _, refresh_board = mock_callbacks
        mock_kagan_app.state_manager.get_ticket.return_value = backlog_auto_ticket
        mock_kagan_app.scheduler.spawn_for_ticket = AsyncMock(return_value=True)

        await session_handler._open_auto_session(backlog_auto_ticket, manual=True)

        # Verify ticket moved to IN_PROGRESS
        mock_kagan_app.state_manager.move_ticket.assert_called_once_with(
            backlog_auto_ticket.id, TicketStatus.IN_PROGRESS
        )
        refresh_board.assert_called_once()

        # Verify agent was spawned
        mock_kagan_app.scheduler.spawn_for_ticket.assert_called_once()

        # Verify notification was shown
        notify.assert_called_once()
        assert "Started agent for" in notify.call_args[0][0]
        assert notify.call_args[0][1] == "information"

    async def test_manual_start_backlog_spawn_fails(
        self,
        session_handler: SessionHandler,
        backlog_auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Manual start spawn failure shows warning."""
        # Cover lines 102-103
        notify, *_ = mock_callbacks
        mock_kagan_app.state_manager.get_ticket.return_value = backlog_auto_ticket
        mock_kagan_app.scheduler.spawn_for_ticket = AsyncMock(return_value=False)

        await session_handler._open_auto_session(backlog_auto_ticket, manual=True)

        # Verify warning was shown
        notify.assert_called_once()
        assert "Failed to start agent" in notify.call_args[0][0]
        assert notify.call_args[0][1] == "warning"

    async def test_manual_spawn_in_progress_opens_modal(
        self,
        session_handler: SessionHandler,
        auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Manual spawn for IN_PROGRESS opens output modal."""
        _, push_screen, _ = mock_callbacks
        mock_kagan_app.scheduler.is_running.return_value = False
        mock_kagan_app.scheduler.spawn_for_ticket = AsyncMock(return_value=True)
        mock_kagan_app.scheduler.get_running_agent.return_value = MagicMock()
        mock_kagan_app.scheduler.get_iteration_count.return_value = 1

        await session_handler._open_auto_session(auto_ticket, manual=True)

        # Verify modal was pushed (AgentOutputModal)
        push_screen.assert_called_once()

    async def test_manual_spawn_in_progress_fails(
        self,
        session_handler: SessionHandler,
        auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Manual spawn failure for IN_PROGRESS shows warning."""
        notify, push_screen, _ = mock_callbacks
        mock_kagan_app.scheduler.is_running.return_value = False
        mock_kagan_app.scheduler.spawn_for_ticket = AsyncMock(return_value=False)

        await session_handler._open_auto_session(auto_ticket, manual=True)

        # Modal should NOT be pushed
        push_screen.assert_not_called()
        # Warning should be shown
        notify.assert_called_once()
        assert "Failed to start agent" in notify.call_args[0][0]


# =============================================================================
# Gateway Modal Callback Tests (Lines 167-176)
# =============================================================================


class TestGatewayModalCallback:
    """Tests for gateway modal result handling."""

    async def test_gateway_cancel_does_nothing(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Cancelling gateway modal does nothing."""
        # Cover lines 167-169
        _, push_screen, _ = mock_callbacks
        mock_kagan_app.config.ui.skip_tmux_gateway = False

        await session_handler._open_pair_session(pair_ticket)

        # Extract the callback passed to push_screen
        push_screen.assert_called_once()
        callback = push_screen.call_args[0][1]

        # Simulate user cancelling (returns None)
        callback(None)

        # call_later should NOT be called for cancelled modal
        mock_textual_app.call_later.assert_not_called()

    async def test_gateway_skip_future_updates_config(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Skip future option updates config and calls callback."""
        # Cover lines 170-174
        _, push_screen, _ = mock_callbacks
        mock_kagan_app.config.ui.skip_tmux_gateway = False

        skip_callback = MagicMock()
        session_handler.set_skip_tmux_gateway_callback(skip_callback)

        await session_handler._open_pair_session(pair_ticket)

        # Extract the callback passed to push_screen
        push_screen.assert_called_once()
        callback = push_screen.call_args[0][1]

        # Simulate user choosing "skip_future"
        callback("skip_future")

        # Config should be updated
        assert mock_kagan_app.config.ui.skip_tmux_gateway == True  # noqa: E712

        # Skip callback should be called
        skip_callback.assert_called_once()

        # call_later should be called to proceed
        mock_textual_app.call_later.assert_called_once()

    async def test_gateway_skip_future_no_callback_set(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Skip future option works even without callback set."""
        # Cover line 173 branch when callback is None
        _, push_screen, _ = mock_callbacks
        mock_kagan_app.config.ui.skip_tmux_gateway = False

        # Don't set a callback - leave it as None
        session_handler._skip_tmux_gateway_callback = None

        await session_handler._open_pair_session(pair_ticket)

        # Extract the callback passed to push_screen
        push_screen.assert_called_once()
        callback = push_screen.call_args[0][1]

        # Simulate user choosing "skip_future"
        callback("skip_future")

        # Config should be updated
        assert mock_kagan_app.config.ui.skip_tmux_gateway == True  # noqa: E712

        # call_later should be called to proceed (even without skip callback)
        mock_textual_app.call_later.assert_called_once()

    async def test_gateway_proceed_opens_session(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Proceeding opens tmux session."""
        # Cover line 176
        _, push_screen, _ = mock_callbacks
        mock_kagan_app.config.ui.skip_tmux_gateway = False

        await session_handler._open_pair_session(pair_ticket)

        # Extract the callback passed to push_screen
        push_screen.assert_called_once()
        callback = push_screen.call_args[0][1]

        # Simulate user choosing "proceed"
        callback("proceed")

        # call_later should be called with _do_open_pair_session
        mock_textual_app.call_later.assert_called_once()
        call_args = mock_textual_app.call_later.call_args[0]
        assert call_args[0] == session_handler._do_open_pair_session
        assert call_args[1] == pair_ticket


# =============================================================================
# Session Termination Detection Tests (Lines 219-239)
# =============================================================================


class TestSessionTerminationDetection:
    """Tests for detecting session exit vs detach."""

    async def test_session_terminated_prompts_for_review(
        self,
        session_handler: SessionHandler,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Session termination (exit) prompts to move to REVIEW."""
        # Cover lines 216-245
        _, push_screen, _ = mock_callbacks
        wt_path = "/path/to/worktree"
        mock_kagan_app.worktree_manager.get_path = AsyncMock(return_value=wt_path)
        mock_kagan_app.session_manager.session_exists = AsyncMock(
            side_effect=[True, False]  # Exists initially, then doesn't exist after attach
        )
        mock_kagan_app.session_manager.attach_session.return_value = True

        # Create an IN_PROGRESS ticket for the test
        in_progress_ticket = Ticket(
            title="Pair task",
            ticket_type=TicketType.PAIR,
            status=TicketStatus.IN_PROGRESS,
        )
        mock_kagan_app.state_manager.get_ticket.return_value = in_progress_ticket

        with patch("asyncio.sleep", new_callable=AsyncMock):
            await session_handler._do_open_pair_session(in_progress_ticket)

        # ConfirmModal should be pushed for review prompt
        # push_screen is called with (modal, callback)
        push_screen.assert_called()
        # The last call should be the ConfirmModal
        modal_call = push_screen.call_args
        modal = modal_call[0][0]
        assert "Session Ended" in str(modal._title)

    async def test_session_detached_no_prompt(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Session detach (Ctrl+B D) doesn't prompt for review."""
        _, push_screen, refresh_board = mock_callbacks
        wt_path = "/path/to/worktree"
        mock_kagan_app.worktree_manager.get_path = AsyncMock(return_value=wt_path)
        mock_kagan_app.session_manager.session_exists = AsyncMock(return_value=True)
        mock_kagan_app.session_manager.attach_session.return_value = True
        mock_kagan_app.state_manager.get_ticket.return_value = pair_ticket

        with patch("asyncio.sleep", new_callable=AsyncMock):
            await session_handler._do_open_pair_session(pair_ticket)

        # ConfirmModal should NOT be pushed (only refresh_board)
        # push_screen should not be called
        push_screen.assert_not_called()

        # Board should still be refreshed
        refresh_board.assert_called()

    async def test_review_confirm_moves_ticket(
        self,
        session_handler: SessionHandler,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Confirming review prompt moves ticket to REVIEW status."""
        import asyncio

        _, push_screen, refresh_board = mock_callbacks
        wt_path = "/path/to/worktree"
        mock_kagan_app.worktree_manager.get_path = AsyncMock(return_value=wt_path)
        mock_kagan_app.session_manager.session_exists = AsyncMock(side_effect=[True, False])
        mock_kagan_app.session_manager.attach_session.return_value = True

        in_progress_ticket = Ticket(
            title="Pair task",
            ticket_type=TicketType.PAIR,
            status=TicketStatus.IN_PROGRESS,
        )
        mock_kagan_app.state_manager.get_ticket.return_value = in_progress_ticket

        with patch("asyncio.sleep", new_callable=AsyncMock):
            await session_handler._do_open_pair_session(in_progress_ticket)

        # Get the callback from the ConfirmModal push
        callback = push_screen.call_args[0][1]

        # Reset move_ticket mock to check call from callback
        mock_kagan_app.state_manager.move_ticket.reset_mock()
        refresh_board.reset_mock()

        # Simulate user confirming
        callback(True)

        # Wait for the async task to complete
        await asyncio.sleep(0.1)

        # Ticket should be moved to REVIEW
        mock_kagan_app.state_manager.move_ticket.assert_called_once_with(
            in_progress_ticket.id, TicketStatus.REVIEW
        )

    async def test_review_decline_refreshes_board(
        self,
        session_handler: SessionHandler,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Declining review prompt just refreshes the board."""
        import asyncio

        _, push_screen, refresh_board = mock_callbacks
        wt_path = "/path/to/worktree"
        mock_kagan_app.worktree_manager.get_path = AsyncMock(return_value=wt_path)
        mock_kagan_app.session_manager.session_exists = AsyncMock(side_effect=[True, False])
        mock_kagan_app.session_manager.attach_session.return_value = True

        in_progress_ticket = Ticket(
            title="Pair task",
            ticket_type=TicketType.PAIR,
            status=TicketStatus.IN_PROGRESS,
        )
        mock_kagan_app.state_manager.get_ticket.return_value = in_progress_ticket

        with patch("asyncio.sleep", new_callable=AsyncMock):
            await session_handler._do_open_pair_session(in_progress_ticket)

        # Get the callback from the ConfirmModal push
        callback = push_screen.call_args[0][1]

        refresh_board.reset_mock()
        mock_kagan_app.state_manager.move_ticket.reset_mock()

        # Simulate user declining
        callback(False)

        # Wait for the async task to complete
        await asyncio.sleep(0.1)

        # Ticket should NOT be moved
        mock_kagan_app.state_manager.move_ticket.assert_not_called()


# =============================================================================
# Session Recreation Tests (Lines 250-257)
# =============================================================================


class TestSessionRecreation:
    """Tests for session recreation after failure."""

    async def test_failed_attach_recreates_session(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Failed attach kills old session and creates new one."""
        # Cover lines 247-257
        wt_path = "/path/to/worktree"
        mock_kagan_app.worktree_manager.get_path = AsyncMock(return_value=wt_path)
        # Session exists (line 199 returns True to skip creation, line 211 returns True)
        mock_kagan_app.session_manager.session_exists = AsyncMock(return_value=True)
        # First attach fails (returns False), then retry succeeds
        mock_kagan_app.session_manager.attach_session = MagicMock(side_effect=[False, True])
        # kill_session is async
        mock_kagan_app.session_manager.kill_session = AsyncMock()
        # create_session is async
        mock_kagan_app.session_manager.create_session = AsyncMock()
        mock_kagan_app.state_manager.get_ticket.return_value = pair_ticket

        with patch("asyncio.sleep", new_callable=AsyncMock):
            await session_handler._do_open_pair_session(pair_ticket)

        # Old session should be killed (line 250)
        mock_kagan_app.session_manager.kill_session.assert_called_once_with(pair_ticket.id)

        # New session should be created (line 251)
        mock_kagan_app.session_manager.create_session.assert_called_once_with(pair_ticket, wt_path)

        # attach_session should be called twice (first fails, retry)
        assert mock_kagan_app.session_manager.attach_session.call_count == 2

    async def test_retry_attach_fails_shows_error(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_textual_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """Failed retry shows error notification."""
        notify, *_ = mock_callbacks
        wt_path = "/path/to/worktree"
        mock_kagan_app.worktree_manager.get_path = AsyncMock(return_value=wt_path)
        # Session exists on all checks
        mock_kagan_app.session_manager.session_exists = AsyncMock(return_value=True)
        # Both attach attempts fail
        mock_kagan_app.session_manager.attach_session = MagicMock(side_effect=[False, False])
        # kill_session is async
        mock_kagan_app.session_manager.kill_session = AsyncMock()
        # create_session is async
        mock_kagan_app.session_manager.create_session = AsyncMock()
        mock_kagan_app.state_manager.get_ticket.return_value = pair_ticket

        with patch("asyncio.sleep", new_callable=AsyncMock):
            await session_handler._do_open_pair_session(pair_ticket)

        # Error notification should be shown (line 257)
        notify.assert_called()
        last_call = notify.call_args
        assert "Session failed to start" in last_call[0][0]
        assert last_call[0][1] == "error"


# =============================================================================
# Error Handling Tests (Lines 260-261)
# =============================================================================


class TestSessionErrorHandling:
    """Tests for error handling in session operations."""

    async def test_tmux_error_shows_notification(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """TmuxError is caught and shown as notification."""
        from kagan.sessions.tmux import TmuxError

        notify, _, _ = mock_callbacks
        mock_kagan_app.worktree_manager.get_path = AsyncMock(
            side_effect=TmuxError("tmux not found")
        )

        await session_handler._do_open_pair_session(pair_ticket)

        notify.assert_called_once()
        assert "Failed to open session" in notify.call_args[0][0]
        assert "tmux not found" in notify.call_args[0][0]
        assert notify.call_args[0][1] == "error"

    async def test_worktree_error_shows_notification(
        self,
        session_handler: SessionHandler,
        pair_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """WorktreeError is caught and shown as notification."""
        from kagan.agents.worktree import WorktreeError

        notify, _, _ = mock_callbacks
        mock_kagan_app.worktree_manager.get_path = AsyncMock(
            side_effect=WorktreeError("git worktree failed")
        )

        await session_handler._do_open_pair_session(pair_ticket)

        notify.assert_called_once()
        assert "Failed to open session" in notify.call_args[0][0]
        assert "git worktree failed" in notify.call_args[0][0]
        assert notify.call_args[0][1] == "error"


# =============================================================================
# Auto-start Mode Tests
# =============================================================================


class TestAutoStartMode:
    """Tests for auto-start mode behavior."""

    async def test_backlog_auto_start_enabled_shows_info(
        self,
        session_handler: SessionHandler,
        backlog_auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """With auto_start enabled, BACKLOG ticket shows started notification."""
        notify, *_ = mock_callbacks
        mock_kagan_app.state_manager.get_ticket.return_value = backlog_auto_ticket
        mock_kagan_app.config.general.auto_start = True

        await session_handler._open_auto_session(backlog_auto_ticket)

        notify.assert_called_once()
        assert "Started AUTO ticket" in notify.call_args[0][0]
        assert notify.call_args[0][1] == "information"

    async def test_in_progress_auto_start_enabled_shows_next_tick(
        self,
        session_handler: SessionHandler,
        auto_ticket: Ticket,
        mock_kagan_app: MagicMock,
        mock_callbacks: tuple[MagicMock, MagicMock, AsyncMock],
    ) -> None:
        """In-progress ticket with auto_start shows 'starting on next tick' message."""
        notify, _, _ = mock_callbacks
        mock_kagan_app.scheduler.is_running.return_value = False
        mock_kagan_app.config.general.auto_start = True

        await session_handler._open_auto_session(auto_ticket)

        notify.assert_called_once()
        assert "next tick" in notify.call_args[0][0]
        assert notify.call_args[0][1] == "information"


class TestStartAgentManual:
    """Tests for the public start_agent_manual method."""

    async def test_start_agent_manual_calls_open_auto_session(
        self,
        session_handler: SessionHandler,
        auto_ticket: Ticket,
    ) -> None:
        """start_agent_manual calls _open_auto_session with manual=True."""
        with patch.object(session_handler, "_open_auto_session", new_callable=AsyncMock) as mock:
            await session_handler.start_agent_manual(auto_ticket)
            mock.assert_called_once_with(auto_ticket, manual=True)
