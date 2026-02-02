"""Tests for ReviewModal actions - Part 2."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

import pytest

from tests.helpers.pages import focus_review_ticket, is_on_screen

if TYPE_CHECKING:
    from pytest_mock import MockerFixture

    from kagan.app import KaganApp
    from kagan.ui.modals.review import ReviewModal

pytestmark = pytest.mark.integration


class TestReviewModalActions:
    """Test ReviewModal approve/reject buttons."""

    @pytest.mark.parametrize(
        "button_id,expected_label",
        [
            ("approve-btn", "Approve"),
            ("reject-btn", "Reject"),
        ],
        ids=["approve", "reject"],
    )
    async def test_action_buttons_exist(
        self, e2e_app_with_tickets: KaganApp, button_id: str, expected_label: str
    ):
        """ReviewModal has Approve and Reject buttons."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            from textual.widgets import Button

            button = pilot.app.screen.query_one(f"#{button_id}", Button)
            assert button is not None
            assert expected_label in str(button.label)

    async def test_a_key_approves(self, e2e_app_with_tickets: KaganApp, mocker: MockerFixture):
        """Pressing 'a' in ReviewModal triggers approve."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            ticket = focus_review_ticket(pilot)
            assert ticket is not None
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
            ticket = focus_review_ticket(pilot)
            assert ticket is not None
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()
            assert is_on_screen(pilot, "ReviewModal")

            # Press 'r' inside modal to reject (binding)
            await pilot.press("r")
            await pilot.pause()

            # Modal should close, PAIR ticket moves to IN_PROGRESS
            assert is_on_screen(pilot, "KanbanScreen")

    @pytest.mark.parametrize(
        "button_id,mock_merge",
        [
            ("approve-btn", True),
            ("reject-btn", False),
        ],
        ids=["approve", "reject"],
    )
    async def test_button_focus_and_enter(
        self,
        e2e_app_with_tickets: KaganApp,
        mocker: MockerFixture,
        button_id: str,
        mock_merge: bool,
    ):
        """Focusing buttons and pressing Enter triggers correct action."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            from textual.widgets import Button

            button = pilot.app.screen.query_one(f"#{button_id}", Button)

            if mock_merge:
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

            button.focus()
            await pilot.pause()
            await pilot.press("enter")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")


class TestReviewModalAIGeneration:
    """Test AI review generation flow."""

    async def test_action_generate_review_sets_loading_state(
        self, e2e_app_with_tickets: KaganApp, mocker: MockerFixture
    ):
        """Generate review hides generate button, shows cancel, updates phase badge."""
        from textual.widgets import Button

        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()
            assert is_on_screen(pilot, "ReviewModal")

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

            # Verify loading state: generate hidden, cancel visible, phase is thinking
            gen_btn = modal.query_one("#generate-btn", Button)
            cancel_btn = modal.query_one("#cancel-btn", Button)
            assert gen_btn.has_class("hidden")
            assert not cancel_btn.has_class("hidden")
            assert modal._phase in ("thinking", "streaming")


class TestReviewModalDiffDisplay:
    """Test diff summary display in ReviewModal."""

    async def test_review_modal_displays_diff_summary(
        self, e2e_app_with_tickets: KaganApp, mocker: MockerFixture
    ):
        """ReviewModal displays diff stats from worktree."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()

            # Mock worktree after app starts (worktree_manager initialized)
            mocker.patch.object(
                e2e_app_with_tickets.worktree_manager,
                "get_diff_stats",
                return_value="1 file changed, 10 insertions(+), 2 deletions(-)",
            )

            ticket = focus_review_ticket(pilot)
            assert ticket is not None
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()
            assert is_on_screen(pilot, "ReviewModal")

            from textual.widgets import Static

            diff_stats = pilot.app.screen.query_one("#diff-stats", Static)
            assert diff_stats is not None


class TestReviewModalDismissResults:
    """Test that approve/reject actions return correct results."""

    @pytest.mark.parametrize(
        "action_name,expected_result",
        [
            ("action_approve", "approve"),
            ("action_reject", "reject"),
        ],
        ids=["approve", "reject"],
    )
    async def test_action_dismisses_with_correct_result(
        self,
        e2e_app_with_tickets: KaganApp,
        mocker: MockerFixture,
        action_name: str,
        expected_result: str,
    ):
        """Actions dismiss modal with correct string result."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            ticket = focus_review_ticket(pilot)
            assert ticket is not None
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()
            assert is_on_screen(pilot, "ReviewModal")

            modal = cast("ReviewModal", pilot.app.screen)
            dismiss_spy = mocker.spy(modal, "dismiss")

            # Call action directly
            action = getattr(modal, action_name)
            action()

            dismiss_spy.assert_called_once_with(expected_result)
            await pilot.pause()
