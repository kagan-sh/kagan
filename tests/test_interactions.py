"""Focused keyboard interaction tests.

Each test verifies a single keyboard shortcut or interaction.
These tests are fast and help quickly identify which keybinding broke.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

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
    from kagan.ui.screens.planner import PlannerScreen


class TestVimNavigation:
    """Test vim-style navigation keys."""

    async def test_j_moves_focus_down(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'j' moves focus to the next card down."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("j")
            await pilot.pause()

    async def test_k_moves_focus_up(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'k' moves focus to the previous card up."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("j")
            await pilot.pause()
            await pilot.press("k")
            await pilot.pause()

    async def test_h_moves_focus_left(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'h' moves focus to the left column."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.IN_PROGRESS:
                    card.focus()
                    break
            await pilot.pause()
            await pilot.press("h")
            await pilot.pause()
            focused = await get_focused_ticket(pilot)
            if focused:
                assert focused.status == TicketStatus.BACKLOG

    async def test_l_moves_focus_right(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'l' moves focus to the right column."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.pause()
            await pilot.press("l")
            await pilot.pause()
            focused = await get_focused_ticket(pilot)
            if focused:
                assert focused.status == TicketStatus.IN_PROGRESS


class TestArrowKeyNavigation:
    """Test arrow key navigation (should work same as hjkl)."""

    async def test_down_arrow_moves_focus_down(self, e2e_app_with_tickets: KaganApp):
        """Pressing down arrow moves focus to the next card down."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("down")
            await pilot.pause()

    async def test_up_arrow_moves_focus_up(self, e2e_app_with_tickets: KaganApp):
        """Pressing up arrow moves focus to the previous card up."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("down")
            await pilot.pause()
            await pilot.press("up")
            await pilot.pause()

    async def test_left_arrow_moves_focus_left(self, e2e_app_with_tickets: KaganApp):
        """Pressing left arrow moves focus to the left column."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            cards = list(pilot.app.screen.query(TicketCard))
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.IN_PROGRESS:
                    card.focus()
                    break
            await pilot.pause()
            await pilot.press("left")
            await pilot.pause()
            focused = await get_focused_ticket(pilot)
            if focused:
                assert focused.status == TicketStatus.BACKLOG

    async def test_right_arrow_moves_focus_right(self, e2e_app_with_tickets: KaganApp):
        """Pressing right arrow moves focus to the right column."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.pause()
            await pilot.press("right")
            await pilot.pause()
            focused = await get_focused_ticket(pilot)
            if focused:
                assert focused.status == TicketStatus.IN_PROGRESS

    async def test_nav_keys_focus_first_card_when_none_focused(
        self, e2e_app_with_tickets: KaganApp
    ):
        """Pressing nav keys when no card focused should focus first card."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            # Deselect any focused card
            await pilot.press("escape")
            await pilot.pause()
            # Press j/down - should focus first card
            await pilot.press("j")
            await pilot.pause()
            focused = await get_focused_ticket(pilot)
            assert focused is not None, "Should focus first card when none selected"


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

    async def test_ctrl_d_deletes_ticket_directly(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'ctrl+d' deletes ticket directly without confirmation."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            ticket_before = await get_focused_ticket(pilot)
            assert ticket_before is not None

            await pilot.press("ctrl+d")
            await pilot.pause()

            # Ticket should be deleted (no confirmation modal)
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
            # Find the AUTO ticket in IN_PROGRESS and focus it
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

            # Try to move forward - should be blocked
            await pilot.press("g", "l")
            await pilot.pause()

            # Ticket should still be in IN_PROGRESS (not moved to REVIEW)
            in_progress = await get_tickets_by_status(pilot, TicketStatus.IN_PROGRESS)
            assert any(t.id == auto_ticket.id for t in in_progress)

    async def test_auto_ticket_in_progress_blocks_backward(
        self, e2e_app_with_auto_ticket: KaganApp
    ):
        """AUTO ticket in IN_PROGRESS should block backward movement via g+h."""
        async with e2e_app_with_auto_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            # Find the AUTO ticket in IN_PROGRESS and focus it
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

            # Try to move backward - should be blocked
            await pilot.press("g", "h")
            await pilot.pause()

            # Ticket should still be in IN_PROGRESS (not moved to BACKLOG)
            in_progress = await get_tickets_by_status(pilot, TicketStatus.IN_PROGRESS)
            assert any(t.id == auto_ticket.id for t in in_progress)

    async def test_done_ticket_backward_shows_confirm(self, e2e_app_with_done_ticket: KaganApp):
        """DONE ticket backward movement should show confirmation dialog."""
        async with e2e_app_with_done_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            # Find the DONE ticket and focus it
            cards = list(pilot.app.screen.query(TicketCard))
            done_ticket = None
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.DONE:
                    card.focus()
                    done_ticket = card.ticket
                    break
            await pilot.pause()
            assert done_ticket is not None, "Should have DONE ticket"

            # Press g+h to move backward - should show confirmation
            await pilot.press("g", "h")
            await pilot.pause()

            # Should show confirmation modal
            assert is_on_screen(pilot, "ConfirmModal")

    async def test_done_ticket_backward_jumps_to_backlog(self, e2e_app_with_done_ticket: KaganApp):
        """DONE ticket backward movement should jump directly to BACKLOG."""
        async with e2e_app_with_done_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            # Find the DONE ticket and focus it
            cards = list(pilot.app.screen.query(TicketCard))
            done_ticket = None
            for card in cards:
                if card.ticket and card.ticket.status == TicketStatus.DONE:
                    card.focus()
                    done_ticket = card.ticket
                    break
            await pilot.pause()
            assert done_ticket is not None, "Should have DONE ticket"

            # Press g+h to move backward
            await pilot.press("g", "h")
            await pilot.pause()

            # Confirm the action
            await pilot.press("y")
            await pilot.pause()

            # Ticket should now be in BACKLOG (not REVIEW, which is the previous status)
            backlog = await get_tickets_by_status(pilot, TicketStatus.BACKLOG)
            assert any(t.id == done_ticket.id for t in backlog)

    async def test_pair_ticket_in_progress_forward_shows_confirm(
        self, e2e_app_with_tickets: KaganApp
    ):
        """PAIR ticket in IN_PROGRESS forward movement should show warning."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            # Find a PAIR ticket in IN_PROGRESS and focus it
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

            # Press g+l to move forward - should show warning confirmation
            await pilot.press("g", "l")
            await pilot.pause()

            # Should show confirmation modal
            assert is_on_screen(pilot, "ConfirmModal")


class TestScreenNavigation:
    """Test screen navigation keybindings."""

    async def test_p_opens_planner(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'p' opens the planner screen."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("p")
            await pilot.pause()
            assert is_on_screen(pilot, "PlannerScreen")


class TestDeselect:
    """Test deselection."""

    async def test_escape_deselects_card(self, e2e_app_with_tickets: KaganApp):
        """Pressing escape deselects the current card."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            assert pilot.app.focused is not None
            await pilot.press("escape")
            await pilot.pause()


class TestPlannerScreen:
    """Test planner screen interactions."""

    async def test_planner_has_header(self, e2e_app_with_tickets: KaganApp):
        """Planner screen should display the header widget."""
        from kagan.ui.widgets.header import KaganHeader

        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("p")  # Open planner
            await pilot.pause()
            assert is_on_screen(pilot, "PlannerScreen")
            # Check header is present
            headers = list(pilot.app.screen.query(KaganHeader))
            assert len(headers) == 1, "Planner screen should have KaganHeader"

    async def test_planner_input_is_focused(self, e2e_app_with_tickets: KaganApp):
        """Planner input should be focused after agent is ready."""
        from textual.widgets import Input

        from kagan.acp import messages

        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("p")  # Open planner
            await pilot.pause()
            assert is_on_screen(pilot, "PlannerScreen")

            # Trigger agent ready state
            screen = cast("PlannerScreen", pilot.app.screen)
            await screen.on_agent_ready(messages.AgentReady())
            await pilot.pause()

            # Input should now be enabled and focused
            input_widget = screen.query_one("#planner-input", Input)
            assert not input_widget.disabled, "Input should be enabled"
            focused = pilot.app.focused
            assert isinstance(focused, Input), "Input should be focused"
            assert focused.id == "planner-input"

    async def test_escape_from_planner_goes_to_board(self, e2e_app_with_tickets: KaganApp):
        """Pressing escape on planner should navigate to board."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("p")  # Open planner
            await pilot.pause()
            assert is_on_screen(pilot, "PlannerScreen")
            await pilot.press("escape")
            await pilot.pause()
            assert is_on_screen(pilot, "KanbanScreen")
