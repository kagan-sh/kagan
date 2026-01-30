"""E2E tests for ticket operations (create, view, edit, delete, move).

Tests: n/v/e/ctrl+d keybindings, ticket movement with g+h/g+l.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.database.models import TicketStatus, TicketType
from kagan.ui.widgets.card import TicketCard
from tests.helpers.pages import (
    focus_first_ticket,
    get_focused_ticket,
    get_tickets_by_status,
    is_on_screen,
)

if TYPE_CHECKING:
    from kagan.app import KaganApp

pytestmark = pytest.mark.e2e


class TestTicketOperations:
    """Test ticket operation keybindings."""

    async def test_n_opens_new_ticket_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'n' opens the new ticket modal."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            assert is_on_screen(pilot, "KanbanScreen")
            await pilot.press("n")
            await pilot.pause()
            assert is_on_screen(pilot, "TicketDetailsModal")

    async def test_escape_closes_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing escape closes the new ticket modal."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("n")
            await pilot.pause()
            assert is_on_screen(pilot, "TicketDetailsModal")
            await pilot.press("escape")
            await pilot.pause()
            assert is_on_screen(pilot, "KanbanScreen")

    async def test_v_opens_view_details(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'v' opens ticket details in view mode."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()
            assert is_on_screen(pilot, "TicketDetailsModal")

    async def test_e_opens_edit_mode(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'e' opens ticket details in edit mode."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("e")
            await pilot.pause()
            assert is_on_screen(pilot, "TicketDetailsModal")

    async def test_x_deletes_ticket_directly(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'x' deletes ticket directly without confirmation."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            ticket_before = await get_focused_ticket(pilot)
            assert ticket_before is not None

            await pilot.press("x")
            await pilot.pause()

            tickets = await e2e_app_with_tickets.state_manager.get_all_tickets()
            assert ticket_before.id not in [t.id for t in tickets]


class TestTicketMovement:
    """Test ticket movement keybindings."""

    async def test_g_l_moves_forward(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'g' then 'l' moves ticket to next status."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            ticket_before = await get_focused_ticket(pilot)
            assert ticket_before is not None
            assert ticket_before.status == TicketStatus.BACKLOG
            await pilot.press("g", "l")
            await pilot.pause()
            in_progress = await get_tickets_by_status(pilot, TicketStatus.IN_PROGRESS)
            assert any(t.id == ticket_before.id for t in in_progress)

    async def test_g_h_moves_backward(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'g' then 'h' moves ticket to previous status."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.IN_PROGRESS:
                    card.focus()
                    break
            await pilot.pause()
            ticket_before = await get_focused_ticket(pilot)
            assert ticket_before is not None
            await pilot.press("g", "h")
            await pilot.pause()
            backlog = await get_tickets_by_status(pilot, TicketStatus.BACKLOG)
            assert any(t.id == ticket_before.id for t in backlog)


class TestTicketMovementRules:
    """Test ticket movement rules for PAIR/AUTO types."""

    async def test_auto_ticket_in_progress_blocks_forward(self, e2e_app_with_auto_ticket: KaganApp):
        """AUTO ticket in IN_PROGRESS should block forward movement via g+l."""
        async with e2e_app_with_auto_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            auto_ticket = None
            for card in cards:
                if (
                    card.ticket
                    and card.ticket.status == TicketStatus.IN_PROGRESS
                    and card.ticket.ticket_type == TicketType.AUTO
                ):
                    card.focus()
                    auto_ticket = card.ticket
                    break
            await pilot.pause()
            assert auto_ticket is not None, "Should have AUTO ticket in IN_PROGRESS"

            await pilot.press("g", "l")
            await pilot.pause()

            in_progress = await get_tickets_by_status(pilot, TicketStatus.IN_PROGRESS)
            assert any(t.id == auto_ticket.id for t in in_progress)

    async def test_auto_ticket_in_progress_blocks_backward(
        self, e2e_app_with_auto_ticket: KaganApp
    ):
        """AUTO ticket in IN_PROGRESS should block backward movement via g+h."""
        async with e2e_app_with_auto_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            auto_ticket = None
            for card in cards:
                if (
                    card.ticket
                    and card.ticket.status == TicketStatus.IN_PROGRESS
                    and card.ticket.ticket_type == TicketType.AUTO
                ):
                    card.focus()
                    auto_ticket = card.ticket
                    break
            await pilot.pause()
            assert auto_ticket is not None, "Should have AUTO ticket in IN_PROGRESS"

            await pilot.press("g", "h")
            await pilot.pause()

            in_progress = await get_tickets_by_status(pilot, TicketStatus.IN_PROGRESS)
            assert any(t.id == auto_ticket.id for t in in_progress)

    async def test_done_ticket_backward_is_blocked(self, e2e_app_with_done_ticket: KaganApp):
        """DONE ticket backward movement should be blocked (immutable Done state)."""
        async with e2e_app_with_done_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            done_ticket = None
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.DONE:
                    card.focus()
                    done_ticket = card.ticket
                    break
            await pilot.pause()
            assert done_ticket is not None, "Should have DONE ticket"

            await pilot.press("g", "h")
            await pilot.pause()

            # Should remain in Done (no modal, action is disabled)
            done_tickets = await get_tickets_by_status(pilot, TicketStatus.DONE)
            assert any(t.id == done_ticket.id for t in done_tickets)

    async def test_done_ticket_forward_is_blocked(self, e2e_app_with_done_ticket: KaganApp):
        """DONE ticket forward movement should also be blocked."""
        async with e2e_app_with_done_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            done_ticket = None
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.DONE:
                    card.focus()
                    done_ticket = card.ticket
                    break
            await pilot.pause()
            assert done_ticket is not None, "Should have DONE ticket"

            await pilot.press("g", "l")
            await pilot.pause()

            # Should remain in Done (already at final status)
            done_tickets = await get_tickets_by_status(pilot, TicketStatus.DONE)
            assert any(t.id == done_ticket.id for t in done_tickets)

    async def test_pair_ticket_in_progress_forward_shows_confirm(
        self, e2e_app_with_tickets: KaganApp
    ):
        """PAIR ticket in IN_PROGRESS forward movement should show warning."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            pair_ticket = None
            for card in cards:
                if (
                    card.ticket
                    and card.ticket.status == TicketStatus.IN_PROGRESS
                    and card.ticket.ticket_type == TicketType.PAIR
                ):
                    card.focus()
                    pair_ticket = card.ticket
                    break
            await pilot.pause()
            assert pair_ticket is not None, "Should have PAIR ticket in IN_PROGRESS"

            await pilot.press("g", "l")
            await pilot.pause()

            assert is_on_screen(pilot, "ConfirmModal")


class TestDoneTicketRestrictions:
    """Test immutable Done state restrictions."""

    async def test_done_ticket_edit_is_blocked(self, e2e_app_with_done_ticket: KaganApp):
        """Pressing 'e' on Done ticket should not open edit mode."""
        async with e2e_app_with_done_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            done_ticket = None
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.DONE:
                    card.focus()
                    done_ticket = card.ticket
                    break
            await pilot.pause()
            assert done_ticket is not None, "Should have DONE ticket"

            await pilot.press("e")
            await pilot.pause()

            # Should not open modal (action is disabled)
            assert not is_on_screen(pilot, "TicketDetailsModal")

    async def test_done_ticket_view_still_works(self, e2e_app_with_done_ticket: KaganApp):
        """Pressing 'v' on Done ticket should open view details (read-only)."""
        async with e2e_app_with_done_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.DONE:
                    card.focus()
                    break
            await pilot.pause()

            await pilot.press("v")
            await pilot.pause()

            # View should work for Done tickets
            assert is_on_screen(pilot, "TicketDetailsModal")

    async def test_done_ticket_delete_still_works(self, e2e_app_with_done_ticket: KaganApp):
        """Pressing 'x' on Done ticket should delete it."""
        async with e2e_app_with_done_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            done_ticket = None
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.DONE:
                    card.focus()
                    done_ticket = card.ticket
                    break
            await pilot.pause()
            assert done_ticket is not None

            await pilot.press("x")
            await pilot.pause()

            # Should have deleted the ticket
            tickets = await e2e_app_with_done_ticket.state_manager.get_all_tickets()
            assert done_ticket.id not in [t.id for t in tickets]


class TestDuplicateTicket:
    """Test ticket duplication feature."""

    async def test_y_opens_duplicate_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'y' opens the duplicate ticket modal."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("y")
            await pilot.pause()
            assert is_on_screen(pilot, "DuplicateTicketModal")

    async def test_duplicate_creates_new_ticket_in_backlog(
        self, e2e_app_with_done_ticket: KaganApp
    ):
        """Duplicating a ticket creates a new ticket in BACKLOG."""
        async with e2e_app_with_done_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            initial_count = len(await e2e_app_with_done_ticket.state_manager.get_all_tickets())

            cards = list(pilot.app.screen.query(TicketCard))
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.DONE:
                    card.focus()
                    break
            await pilot.pause()

            await pilot.press("y")
            await pilot.pause()
            assert is_on_screen(pilot, "DuplicateTicketModal")

            # Submit the duplicate form
            await pilot.press("ctrl+s")
            await pilot.pause()

            # Should have created a new ticket
            tickets = await e2e_app_with_done_ticket.state_manager.get_all_tickets()
            assert len(tickets) == initial_count + 1

            # New ticket should be in BACKLOG
            backlog_tickets = [t for t in tickets if t.status == TicketStatus.BACKLOG]
            assert len(backlog_tickets) >= 1

    async def test_duplicate_escape_cancels(self, e2e_app_with_tickets: KaganApp):
        """Pressing escape in duplicate modal cancels without creating ticket."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            initial_count = len(await e2e_app_with_tickets.state_manager.get_all_tickets())

            await focus_first_ticket(pilot)
            await pilot.press("y")
            await pilot.pause()
            assert is_on_screen(pilot, "DuplicateTicketModal")

            await pilot.press("escape")
            await pilot.pause()

            # Should not have created new ticket
            tickets = await e2e_app_with_tickets.state_manager.get_all_tickets()
            assert len(tickets) == initial_count
            assert is_on_screen(pilot, "KanbanScreen")
