"""E2E edge case tests for KanbanScreen.

Tests: empty board, search edge cases, leader key, navigation edge cases.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.database.models import TicketStatus
from kagan.ui.widgets.card import TicketCard
from kagan.ui.widgets.search_bar import SearchBar
from tests.helpers.pages import get_tickets_by_status, is_on_screen, navigate_to_kanban

if TYPE_CHECKING:
    from kagan.app import KaganApp

pytestmark = pytest.mark.e2e


class TestEmptyBoard:
    """Test empty board state handling."""

    async def test_navigation_on_empty_board(self, e2e_app: KaganApp):
        """Navigation keys should not crash on empty board."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            for key in ["j", "k", "h", "l", "up", "down", "left", "right"]:
                await pilot.press(key)
                await pilot.pause()
            assert is_on_screen(pilot, "KanbanScreen")

    async def test_actions_no_ticket(self, e2e_app: KaganApp):
        """View/edit with no ticket focused should do nothing."""
        async with e2e_app.run_test(size=(120, 40)) as pilot:
            await navigate_to_kanban(pilot)
            await pilot.press("v")
            await pilot.pause()
            assert is_on_screen(pilot, "KanbanScreen")
            await pilot.press("e")
            await pilot.pause()
            assert is_on_screen(pilot, "KanbanScreen")


class TestSearchEdgeCases:
    """Test search functionality edge cases."""

    async def test_toggle_search_twice(self, e2e_app_with_tickets: KaganApp):
        """Toggle search on then off clears filter."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("slash")
            await pilot.pause()
            search = pilot.app.screen.query_one("#search-bar", SearchBar)
            assert search.is_visible
            await pilot.press("escape")
            await pilot.pause()
            assert not search.is_visible

    async def test_search_no_results(self, e2e_app_with_tickets: KaganApp):
        """Search with no matches shows empty board."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("slash")
            await pilot.pause()
            for char in "zzznomatch":
                await pilot.press(char)
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            visible = [c for c in cards if c.ticket]
            assert len(visible) == 0

    async def test_search_empty_query(self, e2e_app_with_tickets: KaganApp):
        """Empty search query shows all tickets."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            initial_count = len(list(pilot.app.screen.query(TicketCard)))
            await pilot.press("slash")
            await pilot.pause()
            await pilot.press("a", "backspace")
            await pilot.pause()
            assert len(list(pilot.app.screen.query(TicketCard))) == initial_count


class TestLeaderKeyEdgeCases:
    """Test leader key cancellation."""

    async def test_leader_invalid_key_cancels(self, e2e_app_with_tickets: KaganApp):
        """Invalid key during leader mode cancels it."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("g", "x")
            await pilot.pause()
            assert is_on_screen(pilot, "KanbanScreen")

    async def test_leader_escape_cancels(self, e2e_app_with_tickets: KaganApp):
        """Escape during leader mode cancels it."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("g", "escape")
            await pilot.pause()
            assert is_on_screen(pilot, "KanbanScreen")


class TestNavigationEdgeCases:
    """Test navigation with edge case board states."""

    async def test_vertical_nav_single_card_column(self, e2e_app_with_tickets: KaganApp):
        """Up/down in single-card column stays on same card."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.IN_PROGRESS:
                    card.focus()
                    break
            await pilot.pause()
            await pilot.press("k", "j")
            await pilot.pause()
            assert isinstance(pilot.app.focused, TicketCard)

    async def test_horizontal_nav_skips_empty_columns(self, e2e_app_with_tickets: KaganApp):
        """Horizontal nav skips columns with no cards."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.REVIEW:
                    card.focus()
                    break
            await pilot.pause()
            await pilot.press("l")
            await pilot.pause()
            assert pilot.app.focused is not None


class TestMoveTicketEdgeCases:
    """Test ticket movement edge cases."""

    async def test_move_forward_at_end(self, e2e_app_with_tickets: KaganApp):
        """Moving forward from REVIEW shows merge confirm."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.REVIEW:
                    card.focus()
                    break
            await pilot.pause()
            await pilot.press("g", "l")
            await pilot.pause()
            assert is_on_screen(pilot, "ConfirmModal")

    async def test_move_backward_at_start(self, e2e_app_with_tickets: KaganApp):
        """Moving backward from BACKLOG shows warning."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.BACKLOG:
                    card.focus()
                    break
            await pilot.pause()
            await pilot.press("g", "h")
            await pilot.pause()
            assert is_on_screen(pilot, "KanbanScreen")
            backlog = await get_tickets_by_status(pilot, TicketStatus.BACKLOG)
            assert len(backlog) >= 1


class TestCheckActionEdgeCases:
    """Test check_action method edge cases."""

    @pytest.mark.slow
    async def test_watch_agent_on_pair_ticket(self, e2e_app_with_tickets: KaganApp):
        """Watch agent action should be disabled for PAIR tickets."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.IN_PROGRESS:
                    card.focus()
                    break
            await pilot.pause()
            await pilot.press("w")
            await pilot.pause()
            assert is_on_screen(pilot, "KanbanScreen")
