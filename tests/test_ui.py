"""UI behavior tests for Kagan TUI."""

import pytest

from kagan.app import KaganApp
from kagan.database.models import TicketCreate, TicketStatus
from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.widgets.card import TicketCard


@pytest.fixture
def app():
    return KaganApp(db_path=":memory:")


def get_kanban_screen(app: KaganApp) -> KanbanScreen:
    screen = app.screen
    assert isinstance(screen, KanbanScreen)
    return screen


class TestKeyboardNavigation:
    async def test_vertical_navigation(self, app: KaganApp):
        async with app.run_test(size=(120, 40)) as pilot:
            sm = app.state_manager
            await sm.create_ticket(TicketCreate(title="Task 1", status=TicketStatus.BACKLOG))
            await sm.create_ticket(TicketCreate(title="Task 2", status=TicketStatus.BACKLOG))

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            cards = list(screen.query(TicketCard))
            assert len(cards) >= 2

            cards[0].focus()
            await pilot.pause()

            first_focused = app.focused
            assert first_focused is not None
            assert isinstance(first_focused, TicketCard)

            await pilot.press("j")
            await pilot.pause()

            second_focused = app.focused
            assert second_focused is not None
            assert second_focused != first_focused

            await pilot.press("k")
            await pilot.pause()

            assert app.focused == first_focused

    async def test_horizontal_navigation(self, app: KaganApp):
        async with app.run_test(size=(120, 40)) as pilot:
            sm = app.state_manager
            await sm.create_ticket(TicketCreate(title="Backlog Task", status=TicketStatus.BACKLOG))
            await sm.create_ticket(
                TicketCreate(title="InProgress Task", status=TicketStatus.IN_PROGRESS)
            )

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            backlog_cards = [
                c
                for c in screen.query(TicketCard)
                if c.ticket and c.ticket.status == TicketStatus.BACKLOG
            ]
            assert len(backlog_cards) >= 1

            backlog_cards[0].focus()
            await pilot.pause()

            first_focused = app.focused
            assert first_focused is not None

            await pilot.press("l")
            await pilot.pause()

            second_focused = app.focused
            assert second_focused is not None
            assert second_focused != first_focused

            await pilot.press("h")
            await pilot.pause()

            assert app.focused == first_focused


class TestCreateTicketFlow:
    async def test_create_ticket_via_form(self, app: KaganApp):
        async with app.run_test(size=(120, 40)) as pilot:
            get_kanban_screen(app)
            await pilot.pause()

            await pilot.press("n")
            await pilot.pause()
            await pilot.pause()

            modal = app.screen
            title_input = modal.query_one("#title-input")
            title_input.focus()
            await pilot.pause()

            for char in "Test ticket":
                await pilot.press(char)
            await pilot.pause()

            await pilot.press("ctrl+s")
            await pilot.pause()

            tickets = await app.state_manager.get_all_tickets()
            assert len(tickets) == 1
            assert tickets[0].title == "Test ticket"

    async def test_cancel_create_ticket(self, app: KaganApp):
        async with app.run_test(size=(120, 40)) as pilot:
            get_kanban_screen(app)
            await pilot.pause()

            await pilot.press("n")
            await pilot.pause()

            await pilot.press("escape")
            await pilot.pause()

            tickets = await app.state_manager.get_all_tickets()
            assert len(tickets) == 0


class TestMoveTicket:
    async def test_move_ticket_forward(self, app: KaganApp):
        async with app.run_test(size=(120, 40)) as pilot:
            sm = app.state_manager
            ticket = await sm.create_ticket(
                TicketCreate(title="Move me", status=TicketStatus.BACKLOG)
            )

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            cards = [c for c in screen.query(TicketCard) if c.ticket and c.ticket.id == ticket.id]
            assert len(cards) == 1
            cards[0].focus()
            await pilot.pause()

            assert app.focused is not None

            await pilot.press("right_square_bracket")
            await pilot.pause()

            updated = await sm.get_ticket(ticket.id)
            assert updated is not None
            assert updated.status == TicketStatus.IN_PROGRESS

    async def test_move_ticket_backward(self, app: KaganApp):
        async with app.run_test(size=(120, 40)) as pilot:
            sm = app.state_manager
            ticket = await sm.create_ticket(
                TicketCreate(title="Move back", status=TicketStatus.IN_PROGRESS)
            )

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            cards = [c for c in screen.query(TicketCard) if c.ticket and c.ticket.id == ticket.id]
            assert len(cards) == 1
            cards[0].focus()
            await pilot.pause()

            await pilot.press("left_square_bracket")
            await pilot.pause()

            updated = await sm.get_ticket(ticket.id)
            assert updated is not None
            assert updated.status == TicketStatus.BACKLOG

    async def test_move_at_boundary_does_not_change_status(self, app: KaganApp):
        async with app.run_test(size=(120, 40)) as pilot:
            sm = app.state_manager
            ticket = await sm.create_ticket(TicketCreate(title="At end", status=TicketStatus.DONE))

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            cards = [c for c in screen.query(TicketCard) if c.ticket and c.ticket.id == ticket.id]
            assert len(cards) == 1
            cards[0].focus()
            await pilot.pause()

            await pilot.press("right_square_bracket")
            await pilot.pause()

            updated = await sm.get_ticket(ticket.id)
            assert updated is not None
            assert updated.status == TicketStatus.DONE
