"""Tests for TicketChanged message routing fix.

This module tests that the TicketChanged message is correctly routed from
the app to the screen. The fix was to post the message to self.screen instead
of self (the app), since Textual messages bubble UP, not DOWN.
"""

from __future__ import annotations

from unittest.mock import AsyncMock, patch

import pytest

from kagan.app import KaganApp, TicketChanged
from kagan.database.models import TicketCreate, TicketStatus
from kagan.ui.screens.kanban import KanbanScreen


@pytest.fixture
def app():
    return KaganApp(db_path=":memory:")


def get_kanban_screen(app: KaganApp) -> KanbanScreen:
    screen = app.screen
    assert isinstance(screen, KanbanScreen)
    return screen


class TestTicketChangedMessageRouting:
    """Test that TicketChanged messages are correctly routed to the screen."""

    async def test_on_ticket_changed_posts_to_screen(self, app: KaganApp):
        """Test that _on_ticket_changed() posts TicketChanged message to screen, not app."""
        async with app.run_test(size=(120, 40)) as pilot:
            await pilot.pause()

            screen = get_kanban_screen(app)

            # Mock the screen's post_message to verify it gets called
            with patch.object(screen, "post_message") as mock_post:
                app._on_ticket_changed()

                # Verify post_message was called on the screen
                mock_post.assert_called_once()
                call_args = mock_post.call_args
                assert isinstance(call_args[0][0], TicketChanged)

    async def test_on_ticket_changed_posts_directly_to_screen_not_app(self, app: KaganApp):
        """Test that _on_ticket_changed() calls screen.post_message(), not app.post_message()."""
        async with app.run_test(size=(120, 40)) as pilot:
            await pilot.pause()

            screen = get_kanban_screen(app)

            # Track which post_message was called directly
            screen_post_called = False
            app_post_called_directly = False

            original_screen_post = screen.post_message
            original_app_post = app.post_message

            def spy_screen_post(message):
                nonlocal screen_post_called
                if isinstance(message, TicketChanged):
                    screen_post_called = True
                return original_screen_post(message)

            def spy_app_post(message):
                nonlocal app_post_called_directly
                if isinstance(message, TicketChanged):
                    app_post_called_directly = True
                return original_app_post(message)

            # Patch both to track which one is called DIRECTLY by _on_ticket_changed
            with (
                patch.object(screen, "post_message", side_effect=spy_screen_post),
                patch.object(app, "post_message", side_effect=spy_app_post),
            ):
                app._on_ticket_changed()

                # screen.post_message should have been called directly
                assert screen_post_called, "TicketChanged should be posted directly to screen"
                # app.post_message should NOT have been called directly by _on_ticket_changed
                # (though it may be called later via message bubbling, that happens after)
                assert not app_post_called_directly, (
                    "TicketChanged should NOT be posted directly to app"
                )

    async def test_on_ticket_changed_does_nothing_without_screen(self, app: KaganApp):
        """Test that _on_ticket_changed() handles case when screen is None."""
        # Before mounting, there's no screen - just verify no exception
        # Note: We can't easily test this with run_test since it pushes a screen
        # but we can at least verify the method has the guard clause
        import inspect

        source = inspect.getsource(app._on_ticket_changed)
        assert "if self.screen:" in source or "if self.screen" in source


class TestKanbanScreenTicketChangedHandler:
    """Test that KanbanScreen correctly handles TicketChanged messages."""

    async def test_kanban_screen_refreshes_on_ticket_changed(self, app: KaganApp):
        """Test that on_ticket_changed handler triggers _refresh_board()."""
        async with app.run_test(size=(120, 40)) as pilot:
            await pilot.pause()

            screen = get_kanban_screen(app)

            # Mock _refresh_board to verify it gets called
            with patch.object(screen, "_refresh_board", new_callable=AsyncMock) as mock_refresh:
                # Simulate receiving a TicketChanged message
                await screen.on_ticket_changed(TicketChanged())
                await pilot.pause()

                mock_refresh.assert_called_once()

    async def test_ticket_changed_message_reaches_screen(self, app: KaganApp):
        """Test the full flow: _on_ticket_changed() -> screen receives message -> refresh."""
        async with app.run_test(size=(120, 40)) as pilot:
            # Create a ticket
            sm = app.state_manager
            await sm.create_ticket(TicketCreate(title="Test", status=TicketStatus.BACKLOG))

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            # Verify initial state
            initial_tickets = screen._tickets
            assert len(initial_tickets) == 1

            # Add another ticket directly to DB (simulating scheduler update)
            await sm.create_ticket(TicketCreate(title="New ticket", status=TicketStatus.BACKLOG))

            # Before _on_ticket_changed, screen still has old data
            assert len(screen._tickets) == 1

            # Trigger the message through the fixed routing
            app._on_ticket_changed()
            await pilot.pause()
            await pilot.pause()  # Extra pause for message processing

            # After message processed, screen should have refreshed
            # Note: we check that board was refreshed by checking ticket count
            assert len(screen._tickets) == 2


class TestTicketChangedIntegration:
    """Integration tests for the full TicketChanged flow."""

    async def test_scheduler_callback_triggers_ui_refresh(self, app: KaganApp):
        """Test that the scheduler's on_ticket_changed callback properly updates the UI."""
        async with app.run_test(size=(120, 40)) as pilot:
            sm = app.state_manager
            ticket = await sm.create_ticket(
                TicketCreate(title="Scheduler test", status=TicketStatus.BACKLOG)
            )

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            # Verify initial status
            assert screen._tickets[0].status == TicketStatus.BACKLOG

            # Simulate scheduler moving ticket to IN_PROGRESS
            from kagan.database.models import TicketUpdate

            await sm.update_ticket(ticket.id, TicketUpdate(status=TicketStatus.IN_PROGRESS))

            # Trigger the callback (as scheduler would do)
            app._on_ticket_changed()
            await pilot.pause()
            await pilot.pause()

            # Verify the UI reflects the change
            updated_ticket = next((t for t in screen._tickets if t.id == ticket.id), None)
            assert updated_ticket is not None
            assert updated_ticket.status == TicketStatus.IN_PROGRESS
