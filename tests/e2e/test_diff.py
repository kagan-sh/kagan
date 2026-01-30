"""Tests for the DiffModal component."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.database.models import TicketStatus
from kagan.ui.widgets.card import TicketCard
from tests.helpers.pages import is_on_screen

if TYPE_CHECKING:
    from kagan.app import KaganApp

pytestmark = pytest.mark.e2e


def _focus_review_ticket(pilot) -> bool:
    """Focus a ticket in REVIEW status. Returns True if found."""
    cards = list(pilot.app.screen.query(TicketCard))
    for card in cards:
        if card.ticket and card.ticket.status == TicketStatus.REVIEW:
            card.focus()
            return True
    return False


class TestDiffModalOpen:
    """Test opening DiffModal via leader key."""

    async def test_g_d_opens_diff_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'g' then 'd' on REVIEW ticket opens DiffModal."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            assert _focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("g", "d")
            await pilot.pause()

            assert is_on_screen(pilot, "DiffModal")

    async def test_g_d_no_effect_on_non_review_ticket(self, e2e_app_with_tickets: KaganApp):
        """Leader 'g'+'d' does nothing on non-REVIEW tickets."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            # Focus BACKLOG ticket
            cards = list(pilot.app.screen.query(TicketCard))
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.BACKLOG:
                    card.focus()
                    break
            await pilot.pause()

            await pilot.press("g", "d")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")


class TestDiffModalDisplay:
    """Test DiffModal content display."""

    async def test_modal_shows_ticket_title(self, e2e_app_with_tickets: KaganApp):
        """DiffModal displays the ticket title in header."""
        from kagan.ui.modals.diff import DiffModal

        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            assert _focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("g", "d")
            await pilot.pause()

            modal = pilot.app.screen
            assert isinstance(modal, DiffModal)
            assert "Review task" in modal._title

    async def test_empty_diff_shows_placeholder(self, e2e_app_with_tickets: KaganApp):
        """Empty diff displays '(No diff available)' message."""
        from kagan.ui.modals.diff import DiffModal

        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            assert _focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("g", "d")
            await pilot.pause()

            modal = pilot.app.screen
            assert isinstance(modal, DiffModal)
            # Modal should be showing (content written via on_mount)
            assert is_on_screen(pilot, "DiffModal")


class TestDiffModalClose:
    """Test closing the DiffModal."""

    async def test_escape_closes_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing escape closes the DiffModal."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            assert _focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("g", "d")
            await pilot.pause()
            assert is_on_screen(pilot, "DiffModal")

            await pilot.press("escape")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")
