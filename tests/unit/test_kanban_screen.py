"""Unit tests for KanbanScreen internal methods.

These tests cover callbacks and internal methods that are difficult to test via E2E.
"""

from __future__ import annotations

import tempfile
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from kagan.database.models import Ticket, TicketPriority, TicketStatus, TicketType

# =============================================================================
# Config Persistence Tests (_save_tmux_gateway_preference)
# =============================================================================


class TestSaveTmuxGatewayPreference:
    """Tests for _save_tmux_gateway_preference method."""

    def test_creates_config_when_not_exists(self):
        """Creates config file with [ui] section when file doesn't exist."""
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / ".kagan" / "config.toml"

            # Create mock kagan_app with config_path
            kagan_app = MagicMock()
            kagan_app.config_path = config_path

            # Import and call the method directly
            from kagan.ui.screens.kanban.screen import KanbanScreen

            # Create a minimal screen mock to test the method
            screen = MagicMock(spec=KanbanScreen)
            screen.kagan_app = kagan_app

            # Call the actual method
            KanbanScreen._save_tmux_gateway_preference(screen)

            # Verify file was created
            assert config_path.exists()
            content = config_path.read_text()
            assert "[ui]" in content
            assert "skip_tmux_gateway = true" in content

    def test_adds_ui_section_when_missing(self):
        """Adds [ui] section when config exists but section is missing."""
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / "config.toml"
            config_path.write_text("[general]\nauto_start = false\n")

            kagan_app = MagicMock()
            kagan_app.config_path = config_path

            from kagan.ui.screens.kanban.screen import KanbanScreen

            screen = MagicMock(spec=KanbanScreen)
            screen.kagan_app = kagan_app

            KanbanScreen._save_tmux_gateway_preference(screen)

            content = config_path.read_text()
            assert "[general]" in content
            assert "[ui]" in content
            assert "skip_tmux_gateway = true" in content

    def test_updates_existing_preference(self):
        """Updates skip_tmux_gateway when it exists."""
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / "config.toml"
            config_path.write_text("[ui]\nskip_tmux_gateway = false\n")

            kagan_app = MagicMock()
            kagan_app.config_path = config_path

            from kagan.ui.screens.kanban.screen import KanbanScreen

            screen = MagicMock(spec=KanbanScreen)
            screen.kagan_app = kagan_app

            KanbanScreen._save_tmux_gateway_preference(screen)

            content = config_path.read_text()
            assert "skip_tmux_gateway = true" in content
            assert "skip_tmux_gateway = false" not in content

    def test_adds_preference_to_existing_ui_section(self):
        """Adds skip_tmux_gateway to existing [ui] section."""
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / "config.toml"
            config_path.write_text("[ui]\nother_setting = true\n")

            kagan_app = MagicMock()
            kagan_app.config_path = config_path

            from kagan.ui.screens.kanban.screen import KanbanScreen

            screen = MagicMock(spec=KanbanScreen)
            screen.kagan_app = kagan_app

            KanbanScreen._save_tmux_gateway_preference(screen)

            content = config_path.read_text()
            assert "[ui]" in content
            assert "skip_tmux_gateway = true" in content
            assert "other_setting = true" in content

    def test_handles_content_without_trailing_newline(self):
        """Handles config content without trailing newline."""
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = Path(tmpdir) / "config.toml"
            config_path.write_text("[general]\nauto_start = false")  # No trailing newline

            kagan_app = MagicMock()
            kagan_app.config_path = config_path

            from kagan.ui.screens.kanban.screen import KanbanScreen

            screen = MagicMock(spec=KanbanScreen)
            screen.kagan_app = kagan_app

            KanbanScreen._save_tmux_gateway_preference(screen)

            content = config_path.read_text()
            assert "[ui]" in content
            assert "skip_tmux_gateway = true" in content


# =============================================================================
# Modal Result Callback Tests
# =============================================================================


class TestOnTicketModalResult:
    """Tests for _on_ticket_modal_result callback."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen.kagan_app = MagicMock()
        screen.kagan_app.state_manager = MagicMock()
        screen.kagan_app.state_manager.create_ticket = AsyncMock()
        screen.kagan_app.state_manager.update_ticket = AsyncMock()
        screen._refresh_board = AsyncMock()
        screen.notify = MagicMock()
        screen._editing_ticket_id = None
        screen.action_delete_ticket = MagicMock()
        return screen

    @pytest.mark.asyncio
    async def test_creates_ticket_from_ticket_object(self, mock_screen):
        """Creates ticket when result is a Ticket object."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="New ticket",
            description="Test description",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.BACKLOG,
        )

        await KanbanScreen._on_ticket_modal_result(mock_screen, ticket)

        mock_screen.kagan_app.state_manager.create_ticket.assert_called_once_with(ticket)
        mock_screen._refresh_board.assert_called_once()
        mock_screen.notify.assert_called_once()
        assert "Created ticket" in mock_screen.notify.call_args[0][0]

    @pytest.mark.asyncio
    async def test_updates_ticket_from_dict(self, mock_screen):
        """Updates ticket when result is a dict and editing_ticket_id is set."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        mock_screen._editing_ticket_id = "test-ticket-id"
        updates = {"title": "Updated title", "description": "Updated desc"}

        await KanbanScreen._on_ticket_modal_result(mock_screen, updates)

        mock_screen.kagan_app.state_manager.update_ticket.assert_called_once_with(
            "test-ticket-id", **updates
        )
        mock_screen._refresh_board.assert_called_once()
        mock_screen.notify.assert_called_once()
        assert "Ticket updated" in mock_screen.notify.call_args[0][0]
        editing_id = mock_screen._editing_ticket_id
        assert editing_id is None

    @pytest.mark.asyncio
    async def test_ignores_dict_without_editing_id(self, mock_screen):
        """Ignores dict result when no editing_ticket_id is set."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        mock_screen._editing_ticket_id = None
        updates = {"title": "Updated title"}

        await KanbanScreen._on_ticket_modal_result(mock_screen, updates)

        mock_screen.kagan_app.state_manager.update_ticket.assert_not_called()
        mock_screen._refresh_board.assert_not_called()

    @pytest.mark.asyncio
    async def test_triggers_delete_on_modal_action(self, mock_screen):
        """Triggers delete when result is ModalAction.DELETE."""
        from kagan.ui.modals import ModalAction
        from kagan.ui.screens.kanban.screen import KanbanScreen

        await KanbanScreen._on_ticket_modal_result(mock_screen, ModalAction.DELETE)

        mock_screen.action_delete_ticket.assert_called_once()

    @pytest.mark.asyncio
    async def test_ignores_none_result(self, mock_screen):
        """Does nothing when result is None."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        await KanbanScreen._on_ticket_modal_result(mock_screen, None)

        mock_screen.kagan_app.state_manager.create_ticket.assert_not_called()
        mock_screen.kagan_app.state_manager.update_ticket.assert_not_called()
        mock_screen.action_delete_ticket.assert_not_called()


# =============================================================================
# Rejection Callback Tests
# =============================================================================


class TestApplyRejectionResult:
    """Tests for _apply_rejection_result callback."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen.kagan_app = MagicMock()
        screen._refresh_board = AsyncMock()
        screen.notify = MagicMock()
        return screen

    @pytest.mark.asyncio
    async def test_shelves_on_none_result(self, mock_screen):
        """Shelves ticket when result is None."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
            ticket_type=TicketType.AUTO,
        )

        with patch(
            "kagan.ui.screens.kanban.actions.apply_rejection_feedback", new_callable=AsyncMock
        ) as mock_apply:
            await KanbanScreen._apply_rejection_result(mock_screen, ticket, None)

            mock_apply.assert_called_once_with(mock_screen.kagan_app, ticket, None, "shelve")
            mock_screen._refresh_board.assert_called_once()
            assert "Shelved" in mock_screen.notify.call_args[0][0]

    @pytest.mark.asyncio
    async def test_retries_with_feedback(self, mock_screen):
        """Retries ticket with feedback."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
            ticket_type=TicketType.AUTO,
        )

        with patch(
            "kagan.ui.screens.kanban.actions.apply_rejection_feedback", new_callable=AsyncMock
        ) as mock_apply:
            await KanbanScreen._apply_rejection_result(
                mock_screen, ticket, ("Fix the bug", "retry")
            )

            mock_apply.assert_called_once_with(
                mock_screen.kagan_app, ticket, "Fix the bug", "retry"
            )
            mock_screen._refresh_board.assert_called_once()
            assert "Retrying" in mock_screen.notify.call_args[0][0]

    @pytest.mark.asyncio
    async def test_stages_for_manual_restart(self, mock_screen):
        """Stages ticket for manual restart."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
            ticket_type=TicketType.AUTO,
        )

        with patch(
            "kagan.ui.screens.kanban.actions.apply_rejection_feedback", new_callable=AsyncMock
        ) as mock_apply:
            await KanbanScreen._apply_rejection_result(mock_screen, ticket, ("Needs work", "stage"))

            mock_apply.assert_called_once_with(mock_screen.kagan_app, ticket, "Needs work", "stage")
            mock_screen._refresh_board.assert_called_once()
            assert "Staged for manual restart" in mock_screen.notify.call_args[0][0]


# =============================================================================
# Handle Reject With Feedback Tests
# =============================================================================


class TestHandleRejectWithFeedback:
    """Tests for _handle_reject_with_feedback method."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen.kagan_app = MagicMock()
        screen.kagan_app.state_manager = MagicMock()
        screen.kagan_app.state_manager.move_ticket = AsyncMock()
        screen.app = MagicMock()
        screen.app.push_screen = AsyncMock()
        screen._refresh_board = AsyncMock()
        screen.notify = MagicMock()
        return screen

    @pytest.mark.asyncio
    async def test_opens_rejection_modal_for_auto_ticket(self, mock_screen):
        """Opens rejection modal for AUTO ticket."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Auto ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
            ticket_type=TicketType.AUTO,
        )

        await KanbanScreen._handle_reject_with_feedback(mock_screen, ticket)

        mock_screen.app.push_screen.assert_called_once()
        # Verify it's a RejectionInputModal
        call_args = mock_screen.app.push_screen.call_args
        from kagan.ui.modals import RejectionInputModal

        assert isinstance(call_args[0][0], RejectionInputModal)

    @pytest.mark.asyncio
    async def test_moves_pair_ticket_to_in_progress(self, mock_screen):
        """Moves PAIR ticket back to IN_PROGRESS without modal."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Pair ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
            ticket_type=TicketType.PAIR,
        )

        await KanbanScreen._handle_reject_with_feedback(mock_screen, ticket)

        mock_screen.kagan_app.state_manager.move_ticket.assert_called_once_with(
            ticket.id, TicketStatus.IN_PROGRESS
        )
        mock_screen._refresh_board.assert_called_once()
        assert "Moved back to IN_PROGRESS" in mock_screen.notify.call_args[0][0]
        # No modal should be opened
        mock_screen.app.push_screen.assert_not_called()


# =============================================================================
# Merge Confirmation Tests
# =============================================================================


class TestOnMergeConfirmed:
    """Tests for _on_merge_confirmed callback."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen.kagan_app = MagicMock()
        screen._pending_merge_ticket = None
        screen._refresh_board = AsyncMock()
        screen.notify = MagicMock()
        return screen

    @pytest.mark.asyncio
    async def test_merge_success(self, mock_screen):
        """Successful merge on confirmation."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )
        mock_screen._pending_merge_ticket = ticket

        with patch(
            "kagan.ui.screens.kanban.actions.merge_ticket", new_callable=AsyncMock
        ) as mock_merge:
            mock_merge.return_value = (True, "Success")

            await KanbanScreen._on_merge_confirmed(mock_screen, True)

            mock_merge.assert_called_once_with(mock_screen.kagan_app, ticket)
            mock_screen._refresh_board.assert_called_once()
            assert "Merged and completed" in mock_screen.notify.call_args[0][0]
            assert mock_screen._pending_merge_ticket is None

    @pytest.mark.asyncio
    async def test_merge_failure_shows_error(self, mock_screen):
        """Failed merge shows error notification."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )
        mock_screen._pending_merge_ticket = ticket

        with patch(
            "kagan.ui.screens.kanban.actions.merge_ticket", new_callable=AsyncMock
        ) as mock_merge:
            mock_merge.return_value = (False, "Merge conflict")

            await KanbanScreen._on_merge_confirmed(mock_screen, True)

            mock_screen.notify.assert_called_once_with("Merge conflict", severity="error")
            assert mock_screen._pending_merge_ticket is None

    @pytest.mark.asyncio
    async def test_cancelled_clears_pending(self, mock_screen):
        """Cancelled confirmation clears pending ticket."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )
        mock_screen._pending_merge_ticket = ticket

        await KanbanScreen._on_merge_confirmed(mock_screen, False)

        assert mock_screen._pending_merge_ticket is None
        mock_screen._refresh_board.assert_not_called()


# =============================================================================
# Advance Confirmation Tests
# =============================================================================


class TestOnAdvanceConfirmed:
    """Tests for _on_advance_confirmed callback."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen.kagan_app = MagicMock()
        screen.kagan_app.state_manager = MagicMock()
        screen.kagan_app.state_manager.move_ticket = AsyncMock()
        screen._pending_advance_ticket = None
        screen._refresh_board = AsyncMock()
        screen.notify = MagicMock()
        return screen

    @pytest.mark.asyncio
    async def test_advance_success(self, mock_screen):
        """Successful advance moves ticket to REVIEW."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.IN_PROGRESS,
        )
        mock_screen._pending_advance_ticket = ticket

        with patch("kagan.ui.screens.kanban.focus.focus_column") as mock_focus:
            await KanbanScreen._on_advance_confirmed(mock_screen, True)

            mock_screen.kagan_app.state_manager.move_ticket.assert_called_once_with(
                ticket.id, TicketStatus.REVIEW
            )
            mock_screen._refresh_board.assert_called_once()
            assert f"Moved #{ticket.id} to REVIEW" in mock_screen.notify.call_args[0][0]
            mock_focus.assert_called_once()
            assert mock_screen._pending_advance_ticket is None

    @pytest.mark.asyncio
    async def test_cancelled_clears_pending(self, mock_screen):
        """Cancelled confirmation clears pending ticket."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.IN_PROGRESS,
        )
        mock_screen._pending_advance_ticket = ticket

        await KanbanScreen._on_advance_confirmed(mock_screen, False)

        assert mock_screen._pending_advance_ticket is None
        mock_screen._refresh_board.assert_not_called()


# =============================================================================
# Delete Confirmation Tests
# =============================================================================


class TestOnDeleteConfirmed:
    """Tests for _on_delete_confirmed callback."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen.kagan_app = MagicMock()
        screen._pending_delete_ticket = None
        screen._refresh_board = AsyncMock()
        screen.notify = MagicMock()
        return screen

    @pytest.mark.asyncio
    async def test_delete_success(self, mock_screen):
        """Successful delete removes ticket."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.BACKLOG,
        )
        mock_screen._pending_delete_ticket = ticket

        with patch(
            "kagan.ui.screens.kanban.actions.delete_ticket", new_callable=AsyncMock
        ) as mock_delete:
            with patch("kagan.ui.screens.kanban.focus.focus_first_card") as mock_focus:
                await KanbanScreen._on_delete_confirmed(mock_screen, True)

                mock_delete.assert_called_once_with(mock_screen.kagan_app, ticket)
                mock_screen._refresh_board.assert_called_once()
                assert "Deleted ticket" in mock_screen.notify.call_args[0][0]
                mock_focus.assert_called_once()
                assert mock_screen._pending_delete_ticket is None

    @pytest.mark.asyncio
    async def test_cancelled_clears_pending(self, mock_screen):
        """Cancelled confirmation clears pending ticket."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.BACKLOG,
        )
        mock_screen._pending_delete_ticket = ticket

        await KanbanScreen._on_delete_confirmed(mock_screen, False)

        assert mock_screen._pending_delete_ticket is None


# =============================================================================
# Diff Result Tests
# =============================================================================


class TestOnDiffResult:
    """Tests for _on_diff_result callback."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen.kagan_app = MagicMock()
        screen._refresh_board = AsyncMock()
        screen.notify = MagicMock()
        screen._handle_reject_with_feedback = AsyncMock()
        return screen

    @pytest.mark.asyncio
    async def test_approve_merges_ticket(self, mock_screen):
        """Approve result triggers merge."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )

        with patch(
            "kagan.ui.screens.kanban.actions.merge_ticket", new_callable=AsyncMock
        ) as mock_merge:
            mock_merge.return_value = (True, "Success")

            await KanbanScreen._on_diff_result(mock_screen, ticket, "approve")

            mock_merge.assert_called_once_with(mock_screen.kagan_app, ticket)
            assert "Merged" in mock_screen.notify.call_args[0][0]

    @pytest.mark.asyncio
    async def test_reject_triggers_feedback_flow(self, mock_screen):
        """Reject result triggers feedback flow."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )

        await KanbanScreen._on_diff_result(mock_screen, ticket, "reject")

        mock_screen._handle_reject_with_feedback.assert_called_once_with(ticket)

    @pytest.mark.asyncio
    async def test_none_result_does_nothing(self, mock_screen):
        """None result does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )

        await KanbanScreen._on_diff_result(mock_screen, ticket, None)

        mock_screen._refresh_board.assert_not_called()
        mock_screen._handle_reject_with_feedback.assert_not_called()


# =============================================================================
# Review Result Tests
# =============================================================================


class TestOnReviewResult:
    """Tests for _on_review_result callback."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen.kagan_app = MagicMock()
        screen._refresh_board = AsyncMock()
        screen.notify = MagicMock()
        screen._handle_reject_with_feedback = AsyncMock()
        return screen

    @pytest.mark.asyncio
    async def test_approve_merges_ticket(self, mock_screen):
        """Approve result triggers merge."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )

        with patch("kagan.ui.screens.kanban.actions.get_review_ticket", return_value=ticket):
            with patch(
                "kagan.ui.screens.kanban.actions.merge_ticket", new_callable=AsyncMock
            ) as mock_merge:
                mock_merge.return_value = (True, "Success")
                with patch(
                    "kagan.ui.screens.kanban.focus.get_focused_card",
                    return_value=MagicMock(ticket=ticket),
                ):
                    await KanbanScreen._on_review_result(mock_screen, "approve")

                    mock_merge.assert_called_once()
                    assert "Merged and completed" in mock_screen.notify.call_args[0][0]

    @pytest.mark.asyncio
    async def test_reject_triggers_feedback_flow(self, mock_screen):
        """Reject result triggers feedback flow."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )

        with patch("kagan.ui.screens.kanban.actions.get_review_ticket", return_value=ticket):
            with patch(
                "kagan.ui.screens.kanban.focus.get_focused_card",
                return_value=MagicMock(ticket=ticket),
            ):
                await KanbanScreen._on_review_result(mock_screen, "reject")

                mock_screen._handle_reject_with_feedback.assert_called_once_with(ticket)

    @pytest.mark.asyncio
    async def test_approve_failure_shows_error(self, mock_screen):
        """Approve with merge failure shows error."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )

        with patch("kagan.ui.screens.kanban.actions.get_review_ticket", return_value=ticket):
            with patch(
                "kagan.ui.screens.kanban.actions.merge_ticket", new_callable=AsyncMock
            ) as mock_merge:
                mock_merge.return_value = (False, "Merge conflict")
                with patch(
                    "kagan.ui.screens.kanban.focus.get_focused_card",
                    return_value=MagicMock(ticket=ticket),
                ):
                    await KanbanScreen._on_review_result(mock_screen, "approve")

                    mock_screen.notify.assert_called_with("Merge conflict", severity="error")

    @pytest.mark.asyncio
    async def test_no_ticket_does_nothing(self, mock_screen):
        """No focused ticket does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        with patch("kagan.ui.screens.kanban.actions.get_review_ticket", return_value=None):
            with patch(
                "kagan.ui.screens.kanban.focus.get_focused_card",
                return_value=MagicMock(ticket=None),
            ):
                await KanbanScreen._on_review_result(mock_screen, "approve")

                mock_screen._refresh_board.assert_not_called()


# =============================================================================
# Merge Direct Tests
# =============================================================================


class TestMergeDirect:
    """Tests for action_merge_direct."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen.kagan_app = MagicMock()
        screen._refresh_board = AsyncMock()
        screen.notify = MagicMock()
        return screen

    @pytest.mark.asyncio
    async def test_merge_direct_success(self, mock_screen):
        """Successful merge direct shows notification."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )

        with patch("kagan.ui.screens.kanban.actions.get_review_ticket", return_value=ticket):
            with patch(
                "kagan.ui.screens.kanban.actions.merge_ticket", new_callable=AsyncMock
            ) as mock_merge:
                mock_merge.return_value = (True, "Success")
                with patch(
                    "kagan.ui.screens.kanban.focus.get_focused_card",
                    return_value=MagicMock(ticket=ticket),
                ):
                    await KanbanScreen.action_merge_direct(mock_screen)

                    mock_merge.assert_called_once()
                    assert "Merged" in mock_screen.notify.call_args[0][0]

    @pytest.mark.asyncio
    async def test_merge_direct_failure(self, mock_screen):
        """Failed merge direct shows error."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )

        with patch("kagan.ui.screens.kanban.actions.get_review_ticket", return_value=ticket):
            with patch(
                "kagan.ui.screens.kanban.actions.merge_ticket", new_callable=AsyncMock
            ) as mock_merge:
                mock_merge.return_value = (False, "Merge conflict")
                with patch(
                    "kagan.ui.screens.kanban.focus.get_focused_card",
                    return_value=MagicMock(ticket=ticket),
                ):
                    await KanbanScreen.action_merge_direct(mock_screen)

                    mock_screen.notify.assert_called_with("Merge conflict", severity="error")

    @pytest.mark.asyncio
    async def test_merge_direct_no_ticket(self, mock_screen):
        """No ticket does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        with patch("kagan.ui.screens.kanban.actions.get_review_ticket", return_value=None):
            with patch(
                "kagan.ui.screens.kanban.focus.get_focused_card",
                return_value=MagicMock(ticket=None),
            ):
                await KanbanScreen.action_merge_direct(mock_screen)

                mock_screen._refresh_board.assert_not_called()


# =============================================================================
# Merge (with confirmation) Tests
# =============================================================================


class TestMerge:
    """Tests for action_merge."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen.kagan_app = MagicMock()
        screen._refresh_board = AsyncMock()
        screen.notify = MagicMock()
        return screen

    @pytest.mark.asyncio
    async def test_merge_failure_shows_error(self, mock_screen):
        """Failed merge shows error."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )

        with patch("kagan.ui.screens.kanban.actions.get_review_ticket", return_value=ticket):
            with patch(
                "kagan.ui.screens.kanban.actions.merge_ticket", new_callable=AsyncMock
            ) as mock_merge:
                mock_merge.return_value = (False, "Merge conflict")
                with patch(
                    "kagan.ui.screens.kanban.focus.get_focused_card",
                    return_value=MagicMock(ticket=ticket),
                ):
                    await KanbanScreen.action_merge(mock_screen)

                    mock_screen.notify.assert_called_with("Merge conflict", severity="error")


# =============================================================================
# Move Ticket Tests
# =============================================================================


class TestMoveTicket:
    """Tests for _move_ticket method."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen.kagan_app = MagicMock()
        screen.kagan_app.state_manager = MagicMock()
        screen.kagan_app.state_manager.move_ticket = AsyncMock()
        screen._refresh_board = AsyncMock()
        screen.notify = MagicMock()
        screen.app = MagicMock()
        screen.app.push_screen = MagicMock()
        screen._pending_merge_ticket = None
        screen._pending_advance_ticket = None
        return screen

    @pytest.mark.asyncio
    async def test_move_no_ticket_does_nothing(self, mock_screen):
        """Move with no ticket does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        with patch("kagan.ui.screens.kanban.focus.get_focused_card", return_value=None):
            await KanbanScreen._move_ticket(mock_screen, forward=True)

            mock_screen._refresh_board.assert_not_called()

    @pytest.mark.asyncio
    async def test_move_no_ticket_in_card_does_nothing(self, mock_screen):
        """Move with card but no ticket does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        mock_card = MagicMock()
        mock_card.ticket = None
        with patch("kagan.ui.screens.kanban.focus.get_focused_card", return_value=mock_card):
            await KanbanScreen._move_ticket(mock_screen, forward=True)

            mock_screen._refresh_board.assert_not_called()


# =============================================================================
# View Diff Tests
# =============================================================================


class TestViewDiff:
    """Tests for action_view_diff."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen.kagan_app = MagicMock()
        screen.kagan_app.worktree_manager = MagicMock()
        screen.kagan_app.worktree_manager.get_diff = AsyncMock(return_value="diff content")
        screen.kagan_app.config = MagicMock()
        screen.kagan_app.config.general = MagicMock()
        screen.kagan_app.config.general.default_base_branch = "main"
        screen.app = MagicMock()
        screen.app.push_screen = AsyncMock()
        screen.notify = MagicMock()
        return screen

    @pytest.mark.asyncio
    async def test_view_diff_no_ticket_does_nothing(self, mock_screen):
        """View diff with no ticket does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        with patch("kagan.ui.screens.kanban.actions.get_review_ticket", return_value=None):
            with patch(
                "kagan.ui.screens.kanban.focus.get_focused_card",
                return_value=MagicMock(ticket=None),
            ):
                await KanbanScreen.action_view_diff(mock_screen)

                mock_screen.app.push_screen.assert_not_called()


# =============================================================================
# Duplicate Result Tests
# =============================================================================


class TestOnDuplicateResult:
    """Tests for _on_duplicate_result callback."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen.kagan_app = MagicMock()
        screen.kagan_app.state_manager = MagicMock()
        screen.kagan_app.state_manager.create_ticket = AsyncMock()
        screen._refresh_board = AsyncMock()
        screen.notify = MagicMock()
        return screen

    @pytest.mark.asyncio
    async def test_duplicate_creates_ticket(self, mock_screen):
        """Duplicate result creates new ticket."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Duplicate ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.BACKLOG,
        )
        mock_screen.kagan_app.state_manager.create_ticket.return_value = ticket

        with patch("kagan.ui.screens.kanban.focus.focus_column"):
            await KanbanScreen._on_duplicate_result(mock_screen, ticket)

            mock_screen.kagan_app.state_manager.create_ticket.assert_called_once_with(ticket)
            mock_screen._refresh_board.assert_called_once()

    @pytest.mark.asyncio
    async def test_duplicate_none_does_nothing(self, mock_screen):
        """Duplicate with None result does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        await KanbanScreen._on_duplicate_result(mock_screen, None)

        mock_screen.kagan_app.state_manager.create_ticket.assert_not_called()


# =============================================================================
# Open Session Tests
# =============================================================================


class TestOpenSession:
    """Tests for action_open_session."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen._session_handler = MagicMock()
        screen._session_handler.open_session = AsyncMock()
        screen.action_open_review = AsyncMock()
        return screen

    @pytest.mark.asyncio
    async def test_open_session_no_ticket_does_nothing(self, mock_screen):
        """Open session with no ticket does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        with patch("kagan.ui.screens.kanban.focus.get_focused_card", return_value=None):
            await KanbanScreen.action_open_session(mock_screen)

            mock_screen._session_handler.open_session.assert_not_called()

    @pytest.mark.asyncio
    async def test_open_session_review_redirects_to_review(self, mock_screen):
        """Open session on REVIEW ticket opens review modal."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        ticket = Ticket.create(
            title="Test ticket",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )

        mock_card = MagicMock()
        mock_card.ticket = ticket
        with patch("kagan.ui.screens.kanban.focus.get_focused_card", return_value=mock_card):
            await KanbanScreen.action_open_session(mock_screen)

            mock_screen.action_open_review.assert_called_once()
            mock_screen._session_handler.open_session.assert_not_called()


# =============================================================================
# Watch/Start/Stop Agent Tests
# =============================================================================


class TestAgentActions:
    """Tests for agent-related actions."""

    @pytest.fixture
    def mock_screen(self):
        """Create a mock screen with all dependencies."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        screen = MagicMock(spec=KanbanScreen)
        screen._agent_controller = MagicMock()
        screen._agent_controller.watch_agent = AsyncMock()
        screen._agent_controller.stop_agent = AsyncMock()
        screen._session_handler = MagicMock()
        screen._session_handler.start_agent_manual = AsyncMock()
        return screen

    @pytest.mark.asyncio
    async def test_watch_agent_no_ticket_does_nothing(self, mock_screen):
        """Watch agent with no ticket does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        with patch("kagan.ui.screens.kanban.focus.get_focused_card", return_value=None):
            await KanbanScreen.action_watch_agent(mock_screen)

            mock_screen._agent_controller.watch_agent.assert_not_called()

    @pytest.mark.asyncio
    async def test_start_agent_no_ticket_does_nothing(self, mock_screen):
        """Start agent with no ticket does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        with patch("kagan.ui.screens.kanban.focus.get_focused_card", return_value=None):
            await KanbanScreen.action_start_agent(mock_screen)

            mock_screen._session_handler.start_agent_manual.assert_not_called()

    @pytest.mark.asyncio
    async def test_stop_agent_no_ticket_does_nothing(self, mock_screen):
        """Stop agent with no ticket does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        with patch("kagan.ui.screens.kanban.focus.get_focused_card", return_value=None):
            await KanbanScreen.action_stop_agent(mock_screen)

            mock_screen._agent_controller.stop_agent.assert_not_called()

    @pytest.mark.asyncio
    async def test_watch_agent_no_controller_does_nothing(self, mock_screen):
        """Watch agent with no controller does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        mock_screen._agent_controller = None
        ticket = Ticket.create(
            title="Test",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.IN_PROGRESS,
            ticket_type=TicketType.AUTO,
        )
        mock_card = MagicMock()
        mock_card.ticket = ticket

        with patch("kagan.ui.screens.kanban.focus.get_focused_card", return_value=mock_card):
            await KanbanScreen.action_watch_agent(mock_screen)
            # Should not raise - just silently do nothing

    @pytest.mark.asyncio
    async def test_start_agent_no_handler_does_nothing(self, mock_screen):
        """Start agent with no handler does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        mock_screen._session_handler = None
        ticket = Ticket.create(
            title="Test",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.IN_PROGRESS,
            ticket_type=TicketType.AUTO,
        )
        mock_card = MagicMock()
        mock_card.ticket = ticket

        with patch("kagan.ui.screens.kanban.focus.get_focused_card", return_value=mock_card):
            await KanbanScreen.action_start_agent(mock_screen)
            # Should not raise - just silently do nothing

    @pytest.mark.asyncio
    async def test_stop_agent_no_controller_does_nothing(self, mock_screen):
        """Stop agent with no controller does nothing."""
        from kagan.ui.screens.kanban.screen import KanbanScreen

        mock_screen._agent_controller = None
        ticket = Ticket.create(
            title="Test",
            description="Test",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.IN_PROGRESS,
            ticket_type=TicketType.AUTO,
        )
        mock_card = MagicMock()
        mock_card.ticket = ticket

        with patch("kagan.ui.screens.kanban.focus.get_focused_card", return_value=mock_card):
            await KanbanScreen.action_stop_agent(mock_screen)
            # Should not raise - just silently do nothing
