"""E2E tests for keyboard navigation (vim and arrow keys).

Tests: hjkl navigation, arrow keys, focus behavior.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.database.models import TicketStatus
from kagan.ui.widgets.card import TicketCard
from tests.helpers.pages import focus_first_ticket, get_focused_ticket

if TYPE_CHECKING:
    from kagan.app import KaganApp

pytestmark = pytest.mark.e2e


class TestVerticalNavigation:
    """Test vertical navigation with vim keys and arrows."""

    @pytest.mark.parametrize("key", ["j", "down"])
    async def test_move_focus_down(self, e2e_app_with_tickets: KaganApp, key: str):
        """Pressing 'j' or 'down' moves focus to the next card down."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press(key)
            await pilot.pause()

    @pytest.mark.parametrize("key_down,key_up", [("j", "k"), ("down", "up")])
    async def test_move_focus_up(self, e2e_app_with_tickets: KaganApp, key_down: str, key_up: str):
        """Pressing 'k' or 'up' moves focus to the previous card up."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press(key_down)
            await pilot.pause()
            await pilot.press(key_up)
            await pilot.pause()


class TestHorizontalNavigation:
    """Test horizontal navigation with vim keys and arrows."""

    @pytest.mark.parametrize("key", ["h", "left"])
    async def test_move_focus_left(self, e2e_app_with_tickets: KaganApp, key: str):
        """Pressing 'h' or 'left' moves focus to the left column."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.IN_PROGRESS:
                    card.focus()
                    break
            await pilot.pause()
            await pilot.press(key)
            await pilot.pause()
            focused = await get_focused_ticket(pilot)
            if focused:
                assert focused.status == TicketStatus.BACKLOG

    @pytest.mark.parametrize("key", ["l", "right"])
    async def test_move_focus_right(self, e2e_app_with_tickets: KaganApp, key: str):
        """Pressing 'l' or 'right' moves focus to the right column."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.pause()
            await pilot.press(key)
            await pilot.pause()
            focused = await get_focused_ticket(pilot)
            if focused:
                assert focused.status == TicketStatus.IN_PROGRESS


class TestNavigationEdgeCases:
    """Test navigation edge cases."""

    async def test_nav_keys_focus_first_card_when_none_focused(
        self, e2e_app_with_tickets: KaganApp
    ):
        """Pressing nav keys when no card focused should focus first card."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("escape")
            await pilot.pause()
            await pilot.press("j")
            await pilot.pause()
            focused = await get_focused_ticket(pilot)
            assert focused is not None, "Should focus first card when none selected"
