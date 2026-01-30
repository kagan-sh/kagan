"""Tests for ReviewModal actions - Part 2."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.database.models import TicketStatus
from kagan.ui.widgets.card import TicketCard
from tests.helpers.pages import is_on_screen

if TYPE_CHECKING:
    from pytest_mock import MockerFixture

    from kagan.app import KaganApp

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
