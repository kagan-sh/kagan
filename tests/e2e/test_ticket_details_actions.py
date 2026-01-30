"""Tests for TicketDetailsModal expand/delete functionality."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

import pytest

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


class TestDescriptionExpand:
    """Test description expand functionality."""

    async def test_f_expands_description_view_mode(self, e2e_app_with_tickets: KaganApp):
        """'f' opens description editor modal in view mode (readonly)."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()

            await pilot.press("f")
            await pilot.pause()

            assert is_on_screen(pilot, "DescriptionEditorModal")

            editor = get_description_editor(pilot)
            assert editor.readonly

    async def test_expand_action_opens_editor_in_edit_mode(self, e2e_app_with_tickets: KaganApp):
        """action_expand_description in edit mode opens editable editor."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("e")
            await pilot.pause()

            # Call action directly since 'f' types into focused input
            modal = get_modal(pilot)
            modal.action_expand_description()
            await pilot.pause()

            assert is_on_screen(pilot, "DescriptionEditorModal")

            editor = get_description_editor(pilot)
            assert not editor.readonly

    async def test_description_editor_returns_to_modal(self, e2e_app_with_tickets: KaganApp):
        """Escape in description editor returns to ticket modal."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()

            await pilot.press("f")
            await pilot.pause()

            assert is_on_screen(pilot, "DescriptionEditorModal")

            await pilot.press("escape")
            await pilot.pause()

            assert is_on_screen(pilot, "TicketDetailsModal")


class TestTicketDetailsDelete:
    """Test delete functionality from modal."""

    async def test_d_triggers_delete_with_confirm(self, e2e_app_with_tickets: KaganApp):
        """Pressing 'd' in view mode triggers delete, requiring confirm."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()

            modal = get_modal(pilot)
            ticket_id = modal.ticket.id if modal.ticket else None
            assert ticket_id is not None

            await pilot.press("d")
            await pilot.pause()

            # After 'd', modal dismisses and shows confirm dialog
            assert is_on_screen(pilot, "ConfirmModal")

            # Confirm deletion
            await pilot.press("y")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")

            tickets = await e2e_app_with_tickets.state_manager.get_all_tickets()
            assert ticket_id not in [t.id for t in tickets]

    async def test_delete_can_be_cancelled(self, e2e_app_with_tickets: KaganApp):
        """Delete confirmation can be cancelled."""
        async with e2e_app_with_tickets.run_test(size=(120, 40)) as pilot:
            await pilot.pause()
            await focus_first_ticket(pilot)
            await pilot.press("v")
            await pilot.pause()

            modal = get_modal(pilot)
            ticket_id = modal.ticket.id if modal.ticket else None
            assert ticket_id is not None

            await pilot.press("d")
            await pilot.pause()

            # Cancel deletion
            await pilot.press("n")
            await pilot.pause()

            assert is_on_screen(pilot, "KanbanScreen")

            # Ticket should still exist
            tickets = await e2e_app_with_tickets.state_manager.get_all_tickets()
            assert ticket_id in [t.id for t in tickets]
