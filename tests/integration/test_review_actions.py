"""Tests for ReviewModal actions - Part 2."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

import pytest

from kagan.database.models import TicketStatus
from kagan.ui.widgets.card import TicketCard
from tests.helpers.pages import is_on_screen

if TYPE_CHECKING:
    from pytest_mock import MockerFixture

    from kagan.app import KaganApp
    from kagan.ui.modals.review import ReviewModal

pytestmark = pytest.mark.integration


def _focus_review_ticket(pilot) -> TicketCard | None:
    """Focus a ticket in REVIEW status. Returns the card or None."""
    cards = list(pilot.app.screen.query(TicketCard))
    for card in cards:
        if card.ticket and card.ticket.status == TicketStatus.REVIEW:
            card.focus()
            return card
    return None


class TestReviewModalActions:
    """Test ReviewModal approve/reject buttons."""

    async def test_approve_button_exists(self, e2e_app_with_tickets: KaganApp):
        """ReviewModal has Approve button."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            _focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            from textual.widgets import Button

            buttons = list(pilot.app.screen.query(Button))
            approve_btn = next((b for b in buttons if b.id == "approve-btn"), None)
            assert approve_btn is not None
            assert "Approve" in str(approve_btn.label)

    async def test_reject_button_exists(self, e2e_app_with_tickets: KaganApp):
        """ReviewModal has Reject button."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            _focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            from textual.widgets import Button

            buttons = list(pilot.app.screen.query(Button))
            reject_btn = next((b for b in buttons if b.id == "reject-btn"), None)
            assert reject_btn is not None
            assert "Reject" in str(reject_btn.label)

    async def test_a_key_approves(self, e2e_app_with_tickets: KaganApp, mocker: MockerFixture):
        """Pressing 'a' in ReviewModal triggers approve."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            card = _focus_review_ticket(pilot)
            assert card is not None
            assert card.ticket is not None
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()
            assert is_on_screen(pilot, "ReviewModal")

            # Mock merge to avoid git operations
            mocker.patch.object(
                e2e_app_with_tickets.worktree_manager,
                "merge_to_main",
                return_value=(True, "Merged"),
            )
            mocker.patch.object(
                e2e_app_with_tickets.worktree_manager,
                "delete",
                new_callable=mocker.AsyncMock,
            )
            mocker.patch.object(
                e2e_app_with_tickets.session_manager,
                "kill_session",
                new_callable=mocker.AsyncMock,
            )
            await pilot.press("a")
            await pilot.pause()

            # Modal should close
            assert is_on_screen(pilot, "KanbanScreen")

    async def test_r_key_rejects(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'r' in ReviewModal triggers reject."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            card = _focus_review_ticket(pilot)
            assert card is not None
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()
            assert is_on_screen(pilot, "ReviewModal")

            # Press 'r' inside modal to reject (binding)
            await pilot.press("r")
            await pilot.pause()

            # Modal should close, PAIR ticket moves to IN_PROGRESS
            assert is_on_screen(pilot, "KanbanScreen")

    async def test_approve_button_focus_and_enter(
        self, e2e_app_with_tickets: KaganApp, mocker: MockerFixture
    ):
        """Focusing Approve button and pressing Enter triggers approve action."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            _focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            from textual.widgets import Button

            approve_btn = pilot.app.screen.query_one("#approve-btn", Button)

            mocker.patch.object(
                e2e_app_with_tickets.worktree_manager,
                "merge_to_main",
                return_value=(True, "Merged"),
            )
            mocker.patch.object(
                e2e_app_with_tickets.worktree_manager,
                "delete",
                new_callable=mocker.AsyncMock,
            )
            mocker.patch.object(
                e2e_app_with_tickets.session_manager,
                "kill_session",
                new_callable=mocker.AsyncMock,
            )
            approve_btn.focus()
            await pilot.pause()
            await pilot.press("enter")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")

    async def test_reject_button_focus_and_enter(self, e2e_app_with_tickets: KaganApp):
        """Focusing Reject button and pressing Enter triggers reject action."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            _focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            from textual.widgets import Button

            reject_btn = pilot.app.screen.query_one("#reject-btn", Button)
            reject_btn.focus()
            await pilot.pause()
            await pilot.press("enter")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")


class TestReviewModalAIGeneration:
    """Test AI review generation flow."""

    async def test_action_generate_review_sets_loading_state(
        self, e2e_app_with_tickets: KaganApp, mocker: MockerFixture
    ):
        """Generate review sets button to disabled with 'Generating...' label."""
        from textual.widgets import Button

        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            _focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()
            assert is_on_screen(pilot, "ReviewModal")

            # The screen IS the modal when a ModalScreen is active
            modal = cast("ReviewModal", pilot.app.screen)

            # Mock worktree to return path and diff
            mocker.patch.object(
                e2e_app_with_tickets.worktree_manager,
                "get_path",
                return_value="/tmp/worktree",
            )
            mocker.patch.object(
                e2e_app_with_tickets.worktree_manager,
                "get_diff",
                return_value="diff --git a/file.py",
            )

            # Mock Agent to prevent actual agent spawn
            mock_agent = mocker.MagicMock()
            mock_agent.start = mocker.MagicMock()
            mock_agent.wait_ready = mocker.AsyncMock()
            mock_agent.send_prompt = mocker.AsyncMock()
            mock_agent.stop = mocker.AsyncMock()
            mocker.patch("kagan.ui.modals.review.Agent", return_value=mock_agent)

            # Trigger generate review action
            await modal.action_generate_review()
            await pilot.pause()

            # Verify loading state
            btn = modal.query_one("#generate-btn", Button)
            assert btn.disabled is True
            assert "Generating" in str(btn.label) or "Complete" in str(btn.label)


class TestReviewModalDiffDisplay:
    """Test diff summary display in ReviewModal."""

    async def test_review_modal_displays_diff_summary(
        self, e2e_app_with_review_ticket_and_worktree: KaganApp
    ):
        """ReviewModal displays diff stats from worktree."""
        async with e2e_app_with_review_ticket_and_worktree.run_test(size=(120, 40)) as pilot:
            await pilot.pause()

            # Focus the REVIEW ticket
            card = _focus_review_ticket(pilot)
            assert card is not None
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()
            assert is_on_screen(pilot, "ReviewModal")

            from textual.widgets import Static

            # Diff stats section should exist and contain changes info
            diff_stats = pilot.app.screen.query_one("#diff-stats", Static)
            # The fixture creates a real worktree with commits, so we expect real diff stats
            content = str(diff_stats.render())
            # Should show some diff info (file changed, insertions, etc.) or "(No changes)"
            assert len(content) > 0


class TestReviewModalDismissResults:
    """Test that approve/reject actions return correct results."""

    async def test_action_approve_returns_approve_string(
        self, e2e_app_with_tickets: KaganApp, mocker: MockerFixture
    ):
        """Approve action dismisses modal with 'approve' string."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            card = _focus_review_ticket(pilot)
            assert card is not None
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()
            assert is_on_screen(pilot, "ReviewModal")

            # The screen IS the modal when a ModalScreen is active
            modal = cast("ReviewModal", pilot.app.screen)

            # Spy on dismiss to capture result
            dismiss_spy = mocker.spy(modal, "dismiss")

            # Call approve action directly
            modal.action_approve()

            # Verify dismiss was called with "approve"
            dismiss_spy.assert_called_once_with("approve")

    async def test_action_reject_returns_reject_string(
        self, e2e_app_with_tickets: KaganApp, mocker: MockerFixture
    ):
        """Reject action dismisses modal with 'reject' string."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            card = _focus_review_ticket(pilot)
            assert card is not None
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()
            assert is_on_screen(pilot, "ReviewModal")

            # The screen IS the modal when a ModalScreen is active
            modal = cast("ReviewModal", pilot.app.screen)

            # Verify the action_reject method returns the correct value by checking dismiss
            dismiss_spy = mocker.spy(modal, "dismiss")

            # Call reject action directly
            modal.action_reject()

            # Verify dismiss was called with "reject"
            dismiss_spy.assert_called_once_with("reject")

            # Wait for modal to close before exiting context
            await pilot.pause()
            await pilot.pause()
