"""Tests for ReviewModal open and display - Part 1."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.database.models import TicketStatus
from kagan.ui.widgets.card import TicketCard
from tests.helpers.pages import is_on_screen

if TYPE_CHECKING:
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


class TestReviewModalOpen:
    """Test opening ReviewModal via different keybindings."""

    async def test_r_opens_review_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'r' on REVIEW ticket opens ReviewModal."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            card = _focus_review_ticket(pilot)
            assert card is not None, "Should have a REVIEW ticket"
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            assert is_on_screen(pilot, "ReviewModal")

    async def test_leader_g_r_opens_review_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'g' then 'r' leader key opens ReviewModal."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            card = _focus_review_ticket(pilot)
            assert card is not None, "Should have a REVIEW ticket"
            await pilot.pause()

            await pilot.press("g", "r")
            await pilot.pause()

            assert is_on_screen(pilot, "ReviewModal")

    async def test_r_on_non_review_ticket_shows_warning(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'r' on non-REVIEW ticket shows warning, not modal."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            # Focus a BACKLOG ticket instead
            cards = list(pilot.app.screen.query(TicketCard))
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.BACKLOG:
                    card.focus()
                    break
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            assert not is_on_screen(pilot, "ReviewModal")
            assert is_on_screen(pilot, "KanbanScreen")

    async def test_enter_on_review_ticket_opens_review_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing Enter on REVIEW ticket opens ReviewModal."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            card = _focus_review_ticket(pilot)
            assert card is not None, "Should have a REVIEW ticket"
            await pilot.pause()

            await pilot.press("enter")
            await pilot.pause()

            assert is_on_screen(pilot, "ReviewModal")


class TestReviewModalDisplay:
    """Test ReviewModal displays commits and diff stats."""

    async def test_modal_shows_ticket_title(self, e2e_app_with_tickets: KaganApp):
        """ReviewModal displays the ticket title."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            card = _focus_review_ticket(pilot)
            assert card is not None
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            labels = list(pilot.app.screen.query(".modal-title"))
            assert len(labels) >= 1, "Modal should have a title label"

    async def test_modal_has_commits_section(self, e2e_app_with_tickets: KaganApp):
        """ReviewModal has commits log section."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            _focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            from textual.widgets import RichLog

            logs = list(pilot.app.screen.query(RichLog))
            assert any(log.id == "commits-log" for log in logs)

    async def test_modal_has_diff_stats_section(self, e2e_app_with_tickets: KaganApp):
        """ReviewModal has diff stats section."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            _focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            from textual.widgets import Static

            statics = list(pilot.app.screen.query(Static))
            assert any(s.id == "diff-stats" for s in statics)

    async def test_modal_has_ai_review_button(self, e2e_app_with_tickets: KaganApp):
        """ReviewModal has Generate AI Review button."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            _focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            from textual.widgets import Button

            buttons = list(pilot.app.screen.query(Button))
            assert any(b.id == "generate-btn" for b in buttons)


class TestReviewModalClose:
    """Test closing ReviewModal."""

    async def test_escape_closes_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing Escape closes ReviewModal without action."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            card = _focus_review_ticket(pilot)
            assert card is not None
            assert card.ticket is not None
            ticket_id = card.ticket.id
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()
            assert is_on_screen(pilot, "ReviewModal")

            await pilot.press("escape")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")
            # Ticket should still be in REVIEW (no action taken)
            tickets = await e2e_app_with_tickets.state_manager.get_all_tickets()
            ticket = next((t for t in tickets if t.id == ticket_id), None)
            assert ticket is not None
            assert ticket.status == TicketStatus.REVIEW

    async def test_escape_does_not_change_ticket_status(self, e2e_app_with_tickets: KaganApp):
        """Escape closes modal without modifying ticket."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            card = _focus_review_ticket(pilot)
            assert card is not None
            assert card.ticket is not None
            original_ticket = card.ticket
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()
            await pilot.press("escape")
            await pilot.pause()

            tickets = await e2e_app_with_tickets.state_manager.get_all_tickets()
            ticket = next((t for t in tickets if t.id == original_ticket.id), None)
            assert ticket is not None
            assert ticket.status == original_ticket.status
            assert ticket.title == original_ticket.title
