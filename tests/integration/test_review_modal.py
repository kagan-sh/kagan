"""Tests for ReviewModal open and display - Part 1."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.database.models import TicketStatus
from kagan.ui.widgets.card import TicketCard
from tests.helpers.pages import focus_review_ticket, is_on_screen

if TYPE_CHECKING:
    from kagan.app import KaganApp

pytestmark = pytest.mark.integration


class TestReviewModalOpen:
    """Test opening ReviewModal via different keybindings."""

    @pytest.mark.parametrize(
        "keys,expected_modal",
        [
            (["r"], True),
            (["g", "r"], True),
            (["enter"], True),
        ],
        ids=["r_key", "leader_g_r", "enter_key"],
    )
    async def test_open_review_modal_on_review_ticket(
        self, e2e_app_with_tickets: KaganApp, keys: list[str], expected_modal: bool
    ):
        """Verify various keybindings open ReviewModal on REVIEW ticket."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            ticket = focus_review_ticket(pilot)
            assert ticket is not None, "Should have a REVIEW ticket"
            await pilot.pause()

            await pilot.press(*keys)
            await pilot.pause()

            assert is_on_screen(pilot, "ReviewModal") == expected_modal

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


class TestReviewModalDisplay:
    """Test ReviewModal displays expected UI elements."""

    @pytest.mark.parametrize(
        "element_id,element_type",
        [
            ("commits-log", "RichLog"),
            ("diff-stats", "Static"),
            ("generate-btn", "Button"),
        ],
        ids=["commits_section", "diff_stats_section", "ai_review_button"],
    )
    async def test_modal_has_expected_elements(
        self, e2e_app_with_tickets: KaganApp, element_id: str, element_type: str
    ):
        """ReviewModal has expected UI elements."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            # Query for element by ID
            element = pilot.app.screen.query_one(f"#{element_id}")
            assert element is not None

    async def test_modal_shows_ticket_title(self, e2e_app_with_tickets: KaganApp):
        """ReviewModal displays the ticket title."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            focus_review_ticket(pilot)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            labels = list(pilot.app.screen.query(".modal-title"))
            assert len(labels) >= 1, "Modal should have a title label"


class TestReviewModalClose:
    """Test closing ReviewModal."""

    async def test_escape_closes_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing Escape closes ReviewModal without action."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            ticket = focus_review_ticket(pilot)
            assert ticket is not None
            ticket_id = ticket.id
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
