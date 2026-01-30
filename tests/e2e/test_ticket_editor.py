"""Tests for TicketEditorScreen."""

from __future__ import annotations

import pytest
from textual.widgets import Button, Input, TextArea

from kagan.app import KaganApp
from kagan.database.models import TicketCreate, TicketPriority, TicketType
from kagan.ui.screens.ticket_editor import TicketEditorScreen

pytestmark = pytest.mark.e2e


@pytest.fixture
def sample_tickets() -> list[TicketCreate]:
    return [
        TicketCreate(
            title="First Ticket",
            description="First description",
            priority=TicketPriority.MEDIUM,
            ticket_type=TicketType.AUTO,
        ),
        TicketCreate(
            title="Second Ticket",
            description="Second description",
            priority=TicketPriority.HIGH,
            ticket_type=TicketType.PAIR,
        ),
    ]


class TestTicketEditorScreen:
    """Tests for TicketEditorScreen."""

    async def test_compose_creates_tabs_for_each_ticket(
        self, sample_tickets: list[TicketCreate]
    ) -> None:
        app = KaganApp(db_path=":memory:")
        async with app.run_test(size=(120, 40)) as pilot:
            screen = TicketEditorScreen(sample_tickets)
            app.push_screen(screen)
            await pilot.pause()
            await pilot.pause()  # Extra pause for compose

            # Check inputs exist for each ticket
            assert screen.query_one("#title-1", Input)
            assert screen.query_one("#title-2", Input)
            assert screen.query_one("#description-1", TextArea)
            assert screen.query_one("#description-2", TextArea)

    async def test_inputs_prepopulated_with_ticket_data(
        self, sample_tickets: list[TicketCreate]
    ) -> None:
        app = KaganApp(db_path=":memory:")
        async with app.run_test(size=(120, 40)) as pilot:
            screen = TicketEditorScreen(sample_tickets)
            app.push_screen(screen)
            await pilot.pause()
            await pilot.pause()

            title1 = screen.query_one("#title-1", Input)
            assert title1.value == "First Ticket"

            desc1 = screen.query_one("#description-1", TextArea)
            assert desc1.text == "First description"

    async def test_escape_dismisses_with_none(self, sample_tickets: list[TicketCreate]) -> None:
        app = KaganApp(db_path=":memory:")
        result = None

        def capture_result(r):
            nonlocal result
            result = r

        async with app.run_test(size=(120, 40)) as pilot:
            screen = TicketEditorScreen(sample_tickets)
            app.push_screen(screen, callback=capture_result)
            await pilot.pause()
            await pilot.pause()
            await pilot.press("escape")
            await pilot.pause()

        assert result is None

    async def test_finish_button_returns_edited_tickets(
        self, sample_tickets: list[TicketCreate]
    ) -> None:
        app = KaganApp(db_path=":memory:")
        result = None

        def capture_result(r):
            nonlocal result
            result = r

        async with app.run_test(size=(120, 40)) as pilot:
            screen = TicketEditorScreen(sample_tickets)
            app.push_screen(screen, callback=capture_result)
            await pilot.pause()
            await pilot.pause()

            # Modify title
            title1 = screen.query_one("#title-1", Input)
            title1.value = "Modified Title"
            await pilot.pause()

            # Click finish button
            finish_btn = screen.query_one("#finish-btn", Button)
            await pilot.click(finish_btn)
            await pilot.pause()

        assert result is not None
        assert len(result) == 2
        assert result[0].title == "Modified Title"

    async def test_ctrl_s_finishes_editing(self, sample_tickets: list[TicketCreate]) -> None:
        app = KaganApp(db_path=":memory:")
        result = None

        def capture_result(r):
            nonlocal result
            result = r

        async with app.run_test(size=(120, 40)) as pilot:
            screen = TicketEditorScreen(sample_tickets)
            app.push_screen(screen, callback=capture_result)
            await pilot.pause()
            await pilot.pause()
            await pilot.press("ctrl+s")
            await pilot.pause()

        assert result is not None
        assert len(result) == 2

    async def test_empty_title_uses_original(self, sample_tickets: list[TicketCreate]) -> None:
        app = KaganApp(db_path=":memory:")
        result = None

        def capture_result(r):
            nonlocal result
            result = r

        async with app.run_test(size=(120, 40)) as pilot:
            screen = TicketEditorScreen(sample_tickets)
            app.push_screen(screen, callback=capture_result)
            await pilot.pause()
            await pilot.pause()

            # Clear title
            title1 = screen.query_one("#title-1", Input)
            title1.value = "   "  # Whitespace only
            await pilot.pause()

            await pilot.press("ctrl+s")
            await pilot.pause()

        assert result is not None
        assert result[0].title == "First Ticket"  # Original preserved

    async def test_focus_first_input_on_mount(self, sample_tickets: list[TicketCreate]) -> None:
        app = KaganApp(db_path=":memory:")
        async with app.run_test(size=(120, 40)) as pilot:
            screen = TicketEditorScreen(sample_tickets)
            app.push_screen(screen)
            await pilot.pause()
            await pilot.pause()

            title1 = screen.query_one("#title-1", Input)
            assert title1.has_focus

    async def test_empty_tickets_list(self) -> None:
        app = KaganApp(db_path=":memory:")
        result = None

        def capture_result(r):
            nonlocal result
            result = r

        async with app.run_test(size=(120, 40)) as pilot:
            screen = TicketEditorScreen([])
            app.push_screen(screen, callback=capture_result)
            await pilot.pause()
            await pilot.pause()
            await pilot.press("ctrl+s")
            await pilot.pause()

        assert result == []

    async def test_preserves_non_editable_fields(self, sample_tickets: list[TicketCreate]) -> None:
        # Add extra fields that aren't editable (use list for acceptance_criteria)
        sample_tickets[0].acceptance_criteria = ["Must pass tests"]
        sample_tickets[0].parent_id = "parent-123"

        app = KaganApp(db_path=":memory:")
        result = None

        def capture_result(r):
            nonlocal result
            result = r

        async with app.run_test(size=(120, 40)) as pilot:
            screen = TicketEditorScreen(sample_tickets)
            app.push_screen(screen, callback=capture_result)
            await pilot.pause()
            await pilot.pause()
            await pilot.press("ctrl+s")
            await pilot.pause()

        assert result is not None
        assert result[0].acceptance_criteria == ["Must pass tests"]
        assert result[0].parent_id == "parent-123"
