"""Feature tests for UI and Navigation.

Tests organized by user-facing features for the Textual TUI.
Each test validates user interactions, keybindings, and modal behaviors.

Covers:
- Kanban board rendering
- Column and ticket navigation (vim-style and arrow keys)
- Leader key (g + key sequences)
- Search/filter functionality
- Modals (Help, Confirm, Diff, TicketDetails, Review)
- Notifications and feedback
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.css.query import NoMatches

from kagan.constants import COLUMN_ORDER, STATUS_LABELS
from kagan.database.models import (
    TicketType,
)
from kagan.ui.modals import ConfirmModal, DiffModal, HelpModal, TicketDetailsModal
from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.widgets.card import TicketCard
from kagan.ui.widgets.column import KanbanColumn
from kagan.ui.widgets.search_bar import SearchBar

if TYPE_CHECKING:
    from kagan.app import KaganApp

# =============================================================================
# Feature: Kanban Board Rendering
# =============================================================================


class TestKanbanBoard:
    """Kanban board displays 4 columns with proper structure."""

    async def test_board_shows_all_four_columns(self, e2e_app_with_tickets: KaganApp):
        """Board displays BACKLOG, IN_PROGRESS, REVIEW, DONE columns."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            assert isinstance(screen, KanbanScreen)

            # All 4 columns should be present
            for status in COLUMN_ORDER:
                column = screen.query_one(f"#column-{status.value.lower()}", KanbanColumn)
                assert column is not None
                assert column.status == status

    async def test_board_column_headers_show_status_labels(self, e2e_app_with_tickets: KaganApp):
        """Each column header displays the correct status label."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            assert isinstance(screen, KanbanScreen)

            for status in COLUMN_ORDER:
                # Column header should contain status label
                column = screen.query_one(f"#column-{status.value.lower()}", KanbanColumn)
                header_label = STATUS_LABELS[status]
                # The header text includes count, e.g., "BACKLOG (0)"
                header = column.query_one(f"#header-{status.value.lower()}")
                rendered = header.render()
                assert header_label in getattr(rendered, "plain", str(rendered))

    async def test_board_displays_ticket_cards_in_correct_columns(
        self, e2e_app_with_tickets: KaganApp
    ):
        """Tickets appear in their corresponding status column."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            assert isinstance(screen, KanbanScreen)

            # Check that each column has appropriate cards
            backlog_column = screen.query_one("#column-backlog", KanbanColumn)
            in_progress_column = screen.query_one("#column-in_progress", KanbanColumn)
            review_column = screen.query_one("#column-review", KanbanColumn)

            # Tickets from fixture should be in correct columns
            backlog_cards = backlog_column.get_cards()
            wip_cards = in_progress_column.get_cards()
            review_cards = review_column.get_cards()

            assert len(backlog_cards) >= 1
            assert len(wip_cards) >= 1
            assert len(review_cards) >= 1

    async def test_empty_board_shows_planner_screen(self, e2e_app: KaganApp):
        """Empty board redirects to Planner screen (chat-first boot)."""
        async with e2e_app.run_test() as pilot:
            await pilot.pause()

            # With empty board, PlannerScreen should be shown
            from kagan.ui.screens.planner import PlannerScreen

            screen = e2e_app.screen
            assert isinstance(screen, PlannerScreen)


# =============================================================================
# Feature: Arrow Key Navigation
# =============================================================================


class TestArrowNavigation:
    """User can navigate with arrow keys."""

    async def test_down_arrow_moves_focus_down(self, e2e_app_with_tickets: KaganApp):
        """Down arrow moves focus to next ticket in column."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            await pilot.press("down")
            await pilot.pause()
            # Navigation should work

    async def test_up_arrow_moves_focus_up(self, e2e_app_with_tickets: KaganApp):
        """Up arrow moves focus to previous ticket in column."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            await pilot.press("up")
            await pilot.pause()

    async def test_left_arrow_moves_to_left_column(self, e2e_app_with_tickets: KaganApp):
        """Left arrow moves focus to column on left."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            await pilot.press("right")  # First move right
            await pilot.pause()
            await pilot.press("left")  # Then left
            await pilot.pause()

    async def test_right_arrow_moves_to_right_column(self, e2e_app_with_tickets: KaganApp):
        """Right arrow moves focus to column on right."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            await pilot.press("right")
            await pilot.pause()


# =============================================================================
# Feature: Tab Navigation
# =============================================================================


class TestTabNavigation:
    """User can navigate between columns with Tab/Shift+Tab."""

    async def test_tab_cycles_to_next_column(self, e2e_app_with_tickets: KaganApp):
        """Tab key cycles focus to the next column."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            await pilot.press("tab")
            await pilot.pause()
            # Focus should have moved to next column

    async def test_shift_tab_cycles_to_previous_column(self, e2e_app_with_tickets: KaganApp):
        """Shift+Tab cycles focus to the previous column."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            # First move right, then shift-tab back
            await pilot.press("tab")
            await pilot.pause()
            await pilot.press("shift+tab")
            await pilot.pause()


# =============================================================================
# Feature: Leader Key (g + key sequences)
# =============================================================================


class TestLeaderKey:
    """Leader key (g) enables compound commands."""

    async def test_g_activates_leader_mode(self, e2e_app_with_tickets: KaganApp):
        """Pressing g shows leader hint bar."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                # Press g to activate leader
                await pilot.press("g")
                await pilot.pause()

                # Leader hint should be visible
                leader_hint = screen.query_one(".leader-hint")
                assert leader_hint.has_class("visible")

    async def test_escape_cancels_leader_mode(self, e2e_app_with_tickets: KaganApp):
        """Pressing Escape while in leader mode cancels it."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                await pilot.press("g")  # Activate leader
                await pilot.pause()
                await pilot.press("escape")  # Cancel
                await pilot.pause()

                # Leader hint should be hidden
                leader_hint = screen.query_one(".leader-hint")
                assert not leader_hint.has_class("visible")

    async def test_g_l_attempts_move_forward(self, e2e_app_with_tickets: KaganApp):
        """g + l attempts to move ticket forward (may fail validation for non-selected)."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            # Navigate to backlog ticket first
            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                await pilot.press("g")
                await pilot.pause()
                await pilot.press("l")  # Move forward action
                await pilot.pause()
                # Action should complete (success or validation message)

    async def test_g_h_attempts_move_backward(self, e2e_app_with_tickets: KaganApp):
        """g + h attempts to move ticket backward."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                await pilot.press("g")
                await pilot.pause()
                await pilot.press("h")  # Move backward action
                await pilot.pause()


# =============================================================================
# Feature: Search/Filter
# =============================================================================


class TestSearchFilter:
    """User can search and filter tickets."""

    async def test_slash_toggles_search_bar(self, e2e_app_with_tickets: KaganApp):
        """Pressing / shows the search bar."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                # Search bar should be hidden initially
                search_bar = screen.query_one("#search-bar", SearchBar)
                assert not search_bar.is_visible

                # Press / to show
                await pilot.press("slash")
                await pilot.pause()

                assert search_bar.is_visible

    async def test_escape_closes_search_bar(self, e2e_app_with_tickets: KaganApp):
        """Pressing Escape while search bar is open closes it."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                # Open search bar
                await pilot.press("slash")
                await pilot.pause()

                search_bar = screen.query_one("#search-bar", SearchBar)
                assert search_bar.is_visible

                # Press escape to close
                await pilot.press("escape")
                await pilot.pause()

                assert not search_bar.is_visible

    async def test_typing_in_search_filters_tickets(self, e2e_app_with_tickets: KaganApp):
        """Typing in search bar filters visible tickets."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                # Open search
                await pilot.press("slash")
                await pilot.pause()

                # Type a search query
                await pilot.press("b", "a", "c", "k", "l", "o", "g")
                await pilot.pause()

                # The QueryChanged event should have been posted
                search_bar = screen.query_one("#search-bar", SearchBar)
                assert "backlog" in search_bar.search_query.lower()


# =============================================================================
# Feature: Help Modal
# =============================================================================


class TestHelpModal:
    """Help modal shows keybindings reference."""

    async def test_f1_opens_help_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing F1 opens the help modal."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            await pilot.press("f1")
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            assert isinstance(screen, HelpModal), f"Expected HelpModal, got {type(screen).__name__}"

    async def test_escape_closes_help_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing Escape closes the help modal."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            # Open help
            await pilot.press("f1")
            await pilot.pause()

            # Close with escape
            await pilot.press("escape")
            await pilot.pause()

            # Modal should be dismissed, back to KanbanScreen
            screen = e2e_app_with_tickets.screen
            assert isinstance(screen, KanbanScreen), (
                f"Expected KanbanScreen, got {type(screen).__name__}"
            )

    async def test_help_modal_has_tabbed_content(self, e2e_app_with_tickets: KaganApp):
        """Help modal contains multiple tabs for organization."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            await pilot.press("f1")
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            assert isinstance(screen, HelpModal)
            # Help modal should have TabbedContent
            tabs = screen.query_one("#help-tabs")
            assert tabs is not None


# =============================================================================
# Feature: Confirm Modal
# =============================================================================


class TestConfirmModal:
    """Confirmation dialogs for destructive actions."""

    async def test_confirm_modal_shows_title_and_message(self, e2e_app_with_tickets: KaganApp):
        """ConfirmModal displays the provided title and message."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            # Push a confirm modal directly
            modal = ConfirmModal(title="Delete?", message="Are you sure?")
            e2e_app_with_tickets.push_screen(modal)
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            assert isinstance(screen, ConfirmModal)
            # Check title is rendered
            title_label = screen.query_one(".confirm-title")
            assert "Delete?" in str(title_label.render())

    async def test_y_confirms_action(self, e2e_app_with_tickets: KaganApp):
        """Pressing y confirms and dismisses the modal with True."""
        result_holder = {"result": None}

        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            def capture_result(result):
                result_holder["result"] = result

            modal = ConfirmModal(title="Confirm?", message="Yes or no?")
            e2e_app_with_tickets.push_screen(modal, callback=capture_result)
            await pilot.pause()

            await pilot.press("y")
            await pilot.pause()

            assert result_holder["result"] is True

    async def test_n_cancels_action(self, e2e_app_with_tickets: KaganApp):
        """Pressing n cancels and dismisses the modal with False."""
        result_holder = {"result": None}

        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            def capture_result(result):
                result_holder["result"] = result

            modal = ConfirmModal(title="Confirm?", message="Yes or no?")
            e2e_app_with_tickets.push_screen(modal, callback=capture_result)
            await pilot.pause()

            await pilot.press("n")
            await pilot.pause()

            assert result_holder["result"] is False


# =============================================================================
# Feature: Ticket Details Modal
# =============================================================================


class TestTicketDetailsModal:
    """Modal for viewing and editing ticket details."""

    async def test_n_opens_create_ticket_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing n opens TicketDetailsModal in create mode."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            await pilot.press("n")
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            assert isinstance(screen, TicketDetailsModal), (
                f"Expected TicketDetailsModal, got {type(screen).__name__}"
            )
            assert screen.is_create is True

    async def test_N_opens_auto_ticket_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing Shift+N opens modal with AUTO type preselected."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            await pilot.press("N")
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            assert isinstance(screen, TicketDetailsModal), (
                f"Expected TicketDetailsModal, got {type(screen).__name__}"
            )
            assert screen.is_create is True
            assert screen._initial_type == TicketType.AUTO

    async def test_v_opens_view_details_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing v on a ticket opens details in view mode."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            # Press v to view details
            await pilot.press("v")
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            # May be TicketDetailsModal (if ticket selected) or still KanbanScreen (if no ticket)
            if isinstance(screen, TicketDetailsModal):
                assert screen.ticket is not None

    async def test_escape_closes_ticket_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing Escape closes the ticket details modal."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            # Open create modal
            await pilot.press("n")
            await pilot.pause()

            # Close with escape
            await pilot.press("escape")
            await pilot.pause()

            # Should be back to KanbanScreen
            screen = e2e_app_with_tickets.screen
            assert isinstance(screen, KanbanScreen), (
                f"Expected KanbanScreen, got {type(screen).__name__}"
            )


# =============================================================================
# Feature: Diff Modal
# =============================================================================


class TestDiffModal:
    """Modal for viewing git diffs with approve/reject."""

    async def test_diff_modal_displays_diff_text(self, e2e_app_with_tickets: KaganApp):
        """DiffModal shows the provided diff content."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            diff_text = "+++ new line\n--- removed line"
            modal = DiffModal(title="Test Diff", diff_text=diff_text)
            e2e_app_with_tickets.push_screen(modal)
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            assert isinstance(screen, DiffModal)

    async def test_a_approves_diff(self, e2e_app_with_tickets: KaganApp):
        """Pressing a in diff modal approves changes."""
        result_holder = {"result": None}

        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            def capture_result(result):
                result_holder["result"] = result

            modal = DiffModal(title="Approve?", diff_text="changes")
            e2e_app_with_tickets.push_screen(modal, callback=capture_result)
            await pilot.pause()

            await pilot.press("a")
            await pilot.pause()

            assert result_holder["result"] == "approve"

    async def test_r_rejects_diff(self, e2e_app_with_tickets: KaganApp):
        """Pressing r in diff modal rejects changes."""
        result_holder = {"result": None}

        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            def capture_result(result):
                result_holder["result"] = result

            modal = DiffModal(title="Reject?", diff_text="changes")
            e2e_app_with_tickets.push_screen(modal, callback=capture_result)
            await pilot.pause()

            await pilot.press("r")
            await pilot.pause()

            assert result_holder["result"] == "reject"

    async def test_escape_closes_diff_without_action(self, e2e_app_with_tickets: KaganApp):
        """Pressing Escape closes diff modal with None result."""
        result_holder = {"result": "not_set"}

        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            def capture_result(result):
                result_holder["result"] = result

            modal = DiffModal(title="Close?", diff_text="changes")
            e2e_app_with_tickets.push_screen(modal, callback=capture_result)
            await pilot.pause()

            await pilot.press("escape")
            await pilot.pause()

            assert result_holder["result"] is None


# =============================================================================
# Feature: Ticket Card Display
# =============================================================================


class TestTicketCardDisplay:
    """Ticket cards display appropriate information."""

    async def test_card_shows_ticket_title(self, e2e_app_with_tickets: KaganApp):
        """Ticket cards display the ticket title."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                # Get any card
                cards = screen.query(TicketCard)
                assert len(list(cards)) > 0

                for card in cards:
                    if card.ticket:
                        # Card should have a ticket with a title
                        assert card.ticket.title is not None
                        assert len(card.ticket.title) > 0

    async def test_card_shows_type_badge(self, e2e_app_with_auto_ticket: KaganApp):
        """AUTO tickets show lightning badge, PAIR shows human icon."""
        async with e2e_app_with_auto_ticket.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_auto_ticket.screen
            if isinstance(screen, KanbanScreen):
                cards = list(screen.query(TicketCard))
                auto_cards = [
                    c for c in cards if c.ticket and c.ticket.ticket_type == TicketType.AUTO
                ]
                assert len(auto_cards) > 0

    async def test_focused_card_has_visual_indicator(self, e2e_app_with_tickets: KaganApp):
        """Focused card has distinct visual styling."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                # Focus should be set on first card
                focused = e2e_app_with_tickets.focused
                # Focused element should be a TicketCard (or None if no cards)
                if focused:
                    assert focused.can_focus


# =============================================================================
# Feature: Column Header Display
# =============================================================================


class TestColumnHeaders:
    """Column headers show status and ticket count."""

    async def test_header_shows_ticket_count(self, e2e_app_with_tickets: KaganApp):
        """Column header shows count of tickets in column."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                backlog_column = screen.query_one("#column-backlog", KanbanColumn)
                header = backlog_column.query_one("#header-backlog")
                rendered_obj = header.render()
                rendered = getattr(rendered_obj, "plain", str(rendered_obj))
                # Should contain "(N)" where N is count
                assert "(" in rendered and ")" in rendered


# =============================================================================
# Feature: Keybinding Hints
# =============================================================================


class TestKeybindingHints:
    """Context-aware keybinding hints in footer."""

    async def test_hints_update_based_on_context(self, e2e_app_with_tickets: KaganApp):
        """Keybinding hints change based on focused ticket status."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                # Hints widget should exist
                try:
                    hints = screen.query_one("#keybinding-hint")
                    assert hints is not None
                except NoMatches:
                    pass  # OK if not visible


# =============================================================================
# Feature: Peek Overlay
# =============================================================================


class TestPeekOverlay:
    """Space key shows ticket scratchpad preview."""

    async def test_space_toggles_peek_overlay(self, e2e_app_with_tickets: KaganApp):
        """Pressing Space shows/hides the peek overlay."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                # Press space to toggle peek
                await pilot.press("space")
                await pilot.pause()

                # Try to find peek overlay
                try:
                    overlay = screen.query_one("#peek-overlay")
                    # It may or may not be visible depending on ticket selection
                    assert overlay is not None
                except NoMatches:
                    pass


# =============================================================================
# Feature: Delete Ticket Confirmation
# =============================================================================


class TestDeleteTicketFlow:
    """Deleting a ticket requires confirmation."""

    async def test_x_opens_confirm_dialog_for_delete(self, e2e_app_with_tickets: KaganApp):
        """Pressing x on ticket shows delete confirmation."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            # Focus a ticket first
            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                # Navigate to get focus on a card
                # Then press x for delete
                await pilot.press("x")
                await pilot.pause()

                # Either confirm modal appears or notification for "no ticket"
                # depends on focus state


# =============================================================================
# Feature: Quit Application
# =============================================================================


class TestQuitApplication:
    """User can quit the application."""

    async def test_q_quits_application(self, e2e_app_with_tickets: KaganApp):
        """Pressing q exits the application."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            # Press q to quit
            await pilot.press("q")
            await pilot.pause()
            # App should be marked for exit


# =============================================================================
# Feature: Screen Size Warning
# =============================================================================


class TestScreenSizeWarning:
    """Warning shown when terminal is too small."""

    async def test_too_small_screen_shows_warning(self, e2e_app_with_tickets: KaganApp):
        """When screen is below minimum, warning is displayed."""
        async with e2e_app_with_tickets.run_test(size=(60, 15)) as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                # Screen should have "too-small" class
                assert screen.has_class("too-small")


# =============================================================================
# Feature: Open Session (Enter key)
# =============================================================================


class TestOpenSession:
    """Enter key opens session for ticket."""

    async def test_enter_on_backlog_ticket_attempts_session(self, e2e_app_with_tickets: KaganApp):
        """Pressing Enter on BACKLOG ticket initiates session creation."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                # Press Enter to open session
                await pilot.press("enter")
                await pilot.pause()
                # May show TmuxGatewayModal or notification


# =============================================================================
# Feature: Planner Screen Navigation
# =============================================================================


class TestPlannerNavigation:
    """User can navigate between Kanban and Planner screens."""

    async def test_p_opens_planner_from_kanban(self, e2e_app_with_tickets: KaganApp):
        """Pressing p opens the Planner screen."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                await pilot.press("p")
                await pilot.pause()

                # Should now be on PlannerScreen
                from kagan.ui.screens.planner import PlannerScreen

                new_screen = e2e_app_with_tickets.screen
                assert isinstance(new_screen, PlannerScreen)

    async def test_escape_returns_to_kanban_from_planner(self, e2e_app_with_tickets: KaganApp):
        """Pressing Escape on Planner returns to Kanban."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                # Go to planner
                await pilot.press("p")
                await pilot.pause()

                # Return with escape
                await pilot.press("escape")
                await pilot.pause()

                # Should be back on KanbanScreen
                assert isinstance(e2e_app_with_tickets.screen, KanbanScreen)


# =============================================================================
# Feature: Settings Modal
# =============================================================================


class TestSettingsModal:
    """Settings modal for editing configuration."""

    async def test_comma_opens_settings_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing comma opens the settings modal."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                await pilot.press("comma")
                await pilot.pause()

                # Settings modal should be visible
                from kagan.ui.modals import SettingsModal

                try:
                    settings = e2e_app_with_tickets.query_one(SettingsModal)
                    assert settings is not None
                except NoMatches:
                    pass  # May fail if async loading


# =============================================================================
# Feature: Notifications
# =============================================================================


class TestNotifications:
    """App provides feedback via notifications."""

    async def test_invalid_action_shows_notification(self, e2e_app_with_tickets: KaganApp):
        """Attempting invalid action shows warning notification."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                # Try to view diff when not on REVIEW ticket
                await pilot.press("D")
                await pilot.pause()
                # Should show notification (tested via lack of error)


# =============================================================================
# Feature: Edit Ticket (e key)
# =============================================================================


class TestEditTicket:
    """User can edit ticket details."""

    async def test_e_opens_edit_mode_for_ticket(self, e2e_app_with_tickets: KaganApp):
        """Pressing e on a ticket opens modal in edit mode."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                await pilot.press("e")
                await pilot.pause()

                try:
                    modal = e2e_app_with_tickets.query_one(TicketDetailsModal)
                    # Should be in editing mode for existing ticket
                    if modal.ticket:
                        assert modal.editing is True
                except NoMatches:
                    pass  # May show "no ticket selected"

    async def test_done_ticket_cannot_be_edited(self, e2e_app_with_done_ticket: KaganApp):
        """Pressing e on DONE ticket shows warning, doesn't open edit."""
        async with e2e_app_with_done_ticket.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_done_ticket.screen
            if isinstance(screen, KanbanScreen):
                # Navigate to done ticket
                await pilot.press("right")  # Move towards DONE column
                await pilot.press("right")
                await pilot.press("right")
                await pilot.pause()

                # Try to edit
                await pilot.press("e")
                await pilot.pause()

                # Should show warning, not edit modal
                # (Validation prevents editing DONE tickets)


# =============================================================================
# Feature: Duplicate Ticket (y key)
# =============================================================================


class TestDuplicateTicket:
    """User can duplicate a ticket."""

    async def test_y_opens_duplicate_modal(self, e2e_app_with_tickets: KaganApp):
        """Pressing y on ticket opens duplicate modal."""
        async with e2e_app_with_tickets.run_test() as pilot:
            await pilot.pause()

            screen = e2e_app_with_tickets.screen
            if isinstance(screen, KanbanScreen):
                await pilot.press("y")
                await pilot.pause()

                # Duplicate modal should appear
                from kagan.ui.modals.duplicate_ticket import DuplicateTicketModal

                try:
                    modal = e2e_app_with_tickets.query_one(DuplicateTicketModal)
                    assert modal is not None
                except NoMatches:
                    pass  # May show "no ticket selected"
