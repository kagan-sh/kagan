"""Tests for TicketDetailsModal view/edit modes."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

import pytest
from textual.widgets import Input, TextArea

from tests.helpers.pages import focus_first_ticket, is_on_screen

if TYPE_CHECKING:
    from kagan.app import KaganApp
    from kagan.ui.modals.description_editor import DescriptionEditorModal
    from kagan.ui.modals.ticket_details.modal import TicketDetailsModal

pytestmark = pytest.mark.e2e


def get_modal(pilot) -> TicketDetailsModal:
    """Get the current screen as TicketDetailsModal."""
    return cast("TicketDetailsModal", pilot.app.screen)


def get_description_editor(pilot) -> DescriptionEditorModal:
    """Get the current screen as DescriptionEditorModal."""
    return cast("DescriptionEditorModal", pilot.app.screen)


class TestTicketDetailsView:
    """Test opening ticket details in view mode."""

    async def test_v_opens_view_mode(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'v' opens modal in view mode (not editing)."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()

            assert is_on_screen(pilot, "TicketDetailsModal")
            modal = get_modal(pilot)
            assert not modal.editing

    async def test_view_mode_shows_ticket_data(self, e2e_app_with_tickets: KaganApp):
        """View mode displays ticket title and description."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()

            modal = get_modal(pilot)
            assert modal.ticket is not None
            assert modal.ticket.title == "Backlog task"

    async def test_view_mode_has_edit_button(self, e2e_app_with_tickets: KaganApp):
        """View mode shows edit button."""
        from textual.widgets import Button

        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()

            edit_btn = pilot.app.screen.query_one("#edit-btn", Button)
            assert edit_btn is not None


class TestTicketDetailsEdit:
    """Test edit mode behavior."""

    async def test_e_opens_edit_mode(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'e' opens modal in edit mode."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("e")
            await pilot.pause()

            assert is_on_screen(pilot, "TicketDetailsModal")
            modal = get_modal(pilot)
            assert modal.editing

    async def test_edit_button_toggles_to_edit_mode(self, e2e_app_with_tickets: KaganApp):
        """Clicking edit button switches from view to edit mode."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()

            modal = get_modal(pilot)
            assert not modal.editing

            await pilot.click("#edit-btn")
            await pilot.pause()

            assert modal.editing

    async def test_e_key_toggles_to_edit_mode(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'e' in view mode switches to edit mode."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()

            modal = get_modal(pilot)
            assert not modal.editing

            await pilot.press("e")
            await pilot.pause()

            assert modal.editing

    async def test_edit_mode_shows_input_fields(self, e2e_app_with_tickets: KaganApp):
        """Edit mode shows input fields for title and description."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("e")
            await pilot.pause()

            title_input = pilot.app.screen.query_one("#title-input", Input)
            desc_input = pilot.app.screen.query_one("#description-input", TextArea)
            assert title_input is not None
            assert desc_input is not None


class TestTicketDetailsSave:
    """Test saving changes."""

    async def test_ctrl_s_saves_changes(self, e2e_app_with_tickets: KaganApp):
        """Ctrl+S saves edited ticket."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("e")
            await pilot.pause()

            title_input = pilot.app.screen.query_one("#title-input", Input)
            title_input.value = "Updated title"
            await pilot.pause()

            await pilot.press("ctrl+s")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")

            tickets = await e2e_app_with_tickets.state_manager.get_all_tickets()
            assert any(t.title == "Updated title" for t in tickets)

    async def test_empty_title_shows_error(self, e2e_app_with_tickets: KaganApp):
        """Empty title prevents save and shows error."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("e")
            await pilot.pause()

            title_input = pilot.app.screen.query_one("#title-input", Input)
            title_input.value = ""
            await pilot.pause()

            await pilot.press("ctrl+s")
            await pilot.pause()

            # Should still be on modal (save failed)
            assert is_on_screen(pilot, "TicketDetailsModal")


class TestTicketDetailsCancel:
    """Test escape/cancel behavior."""

    async def test_escape_in_view_mode_closes(self, e2e_app_with_tickets: KaganApp):
        """Escape in view mode closes the modal."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()

            assert is_on_screen(pilot, "TicketDetailsModal")

            await pilot.press("escape")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")

    async def test_escape_in_edit_mode_cancels_to_view(self, e2e_app_with_tickets: KaganApp):
        """Escape in edit mode returns to view mode (existing ticket)."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("e")
            await pilot.pause()

            modal = get_modal(pilot)
            assert modal.editing

            await pilot.press("escape")
            await pilot.pause()

            assert is_on_screen(pilot, "TicketDetailsModal")
            assert not modal.editing

    async def test_escape_cancels_and_resets_form(self, e2e_app_with_tickets: KaganApp):
        """Escape in edit mode discards unsaved changes."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("e")
            await pilot.pause()

            title_input = pilot.app.screen.query_one("#title-input", Input)
            original = title_input.value
            title_input.value = "Changed title"
            await pilot.pause()

            await pilot.press("escape")
            await pilot.pause()

            assert title_input.value == original

    async def test_close_button_in_view_mode(self, e2e_app_with_tickets: KaganApp):
        """Close button dismisses modal."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()

            await pilot.click("#close-btn")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")


class TestAcceptanceCriteria:
    """Test acceptance criteria display and editing."""

    async def test_ac_displayed_in_view_mode(self, e2e_app_with_ac_ticket: KaganApp):
        """Acceptance criteria displayed in view mode."""
        async with e2e_app_with_ac_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()

            assert is_on_screen(pilot, "TicketDetailsModal")
            modal = get_modal(pilot)
            assert modal.ticket is not None
            assert len(modal.ticket.acceptance_criteria) == 2
            # Verify AC section is rendered (has ac-item widgets)
            ac_section = pilot.app.screen.query("#ac-section")
            assert len(ac_section) == 1

    async def test_ac_editable_in_edit_mode(self, e2e_app_with_ac_ticket: KaganApp):
        """Acceptance criteria TextArea visible in edit mode."""
        async with e2e_app_with_ac_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("e")
            await pilot.pause()

            ac_input = pilot.app.screen.query_one("#ac-input", TextArea)
            assert "User can login" in ac_input.text
            assert "Error messages shown" in ac_input.text

    async def test_ac_saved_on_edit(self, e2e_app_with_ac_ticket: KaganApp):
        """Edited acceptance criteria saved to database."""
        async with e2e_app_with_ac_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("e")
            await pilot.pause()

            ac_input = pilot.app.screen.query_one("#ac-input", TextArea)
            ac_input.text = "New criterion 1\nNew criterion 2\nNew criterion 3"
            await pilot.pause()

            await pilot.press("ctrl+s")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")
            tickets = await e2e_app_with_ac_ticket.state_manager.get_all_tickets()
            ticket = tickets[0]
            assert len(ticket.acceptance_criteria) == 3
            assert "New criterion 1" in ticket.acceptance_criteria

    async def test_ac_count_badge_on_card(self, e2e_app_with_ac_ticket: KaganApp):
        """Ticket card shows [AC:N] badge when criteria exist."""
        async with e2e_app_with_ac_ticket.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            from kagan.ui.widgets.card import TicketCard

            cards = pilot.app.screen.query(TicketCard)
            assert len(cards) >= 1
            card = cards[0]
            # Verify ticket has AC and card is rendered
            assert card.ticket is not None
            assert len(card.ticket.acceptance_criteria) == 2

    async def test_ac_empty_not_displayed(self, e2e_app_with_tickets: KaganApp):
        """No AC section when ticket has no acceptance criteria."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()

            assert is_on_screen(pilot, "TicketDetailsModal")
            ac_items = pilot.app.screen.query(".ac-item")
            assert len(ac_items) == 0

    async def test_ac_create_new_ticket_with_criteria(self, e2e_app_with_tickets: KaganApp):
        """Can create new ticket with acceptance criteria."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await pilot.press("n")
            await pilot.pause()

            assert is_on_screen(pilot, "TicketDetailsModal")
            modal = get_modal(pilot)
            assert modal.is_create

            title_input = pilot.app.screen.query_one("#title-input", Input)
            title_input.value = "New feature ticket"

            ac_input = pilot.app.screen.query_one("#ac-input", TextArea)
            ac_input.text = "Feature works correctly\nTests pass"
            await pilot.pause()

            await pilot.press("ctrl+s")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")
            tickets = await e2e_app_with_tickets.state_manager.get_all_tickets()
            new_ticket = next((t for t in tickets if t.title == "New feature ticket"), None)
            assert new_ticket is not None
            assert len(new_ticket.acceptance_criteria) == 2
            assert "Feature works correctly" in new_ticket.acceptance_criteria
