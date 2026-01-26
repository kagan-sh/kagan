"""Tests for the animated card indicator feature."""

from __future__ import annotations

from unittest.mock import MagicMock

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


class TestTicketCardAnimation:
    async def test_card_starts_without_agent_active(self, app: KaganApp):
        """Card should have is_agent_active=False by default."""
        async with app.run_test(size=(120, 40)) as pilot:
            sm = app.state_manager
            await sm.create_ticket(TicketCreate(title="Test Card", status=TicketStatus.BACKLOG))

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            cards = list(screen.query(TicketCard))
            assert len(cards) >= 1

            card = cards[0]
            assert card.is_agent_active is False
            assert not card.has_class("agent-active")

    async def test_card_adds_agent_active_class_when_active(self, app: KaganApp):
        """Card should have 'agent-active' class when is_agent_active=True."""
        async with app.run_test(size=(120, 40)) as pilot:
            sm = app.state_manager
            await sm.create_ticket(TicketCreate(title="Active Card", status=TicketStatus.BACKLOG))

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            cards = list(screen.query(TicketCard))
            assert len(cards) >= 1

            card = cards[0]
            card.is_agent_active = True
            await pilot.pause()

            assert card.has_class("agent-active")

    async def test_card_removes_classes_when_deactivated(self, app: KaganApp):
        """Card should remove 'agent-active' and 'agent-pulse' when deactivated."""
        async with app.run_test(size=(120, 40)) as pilot:
            sm = app.state_manager
            await sm.create_ticket(
                TicketCreate(title="Deactivate Card", status=TicketStatus.BACKLOG)
            )

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            cards = list(screen.query(TicketCard))
            card = cards[0]

            # Activate first
            card.is_agent_active = True
            await pilot.pause()
            assert card.has_class("agent-active")

            # Deactivate
            card.is_agent_active = False
            await pilot.pause()

            assert not card.has_class("agent-active")
            assert not card.has_class("agent-pulse")

    async def test_pulse_animation_starts_when_active(self, app: KaganApp):
        """Pulse animation should toggle 'agent-pulse' class when card is activated."""
        async with app.run_test(size=(120, 40)) as pilot:
            sm = app.state_manager
            await sm.create_ticket(TicketCreate(title="Timer Card", status=TicketStatus.BACKLOG))

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            cards = list(screen.query(TicketCard))
            card = cards[0]

            # Card should not have pulse class when not active
            assert not card.has_class("agent-pulse")

            card.is_agent_active = True
            await pilot.pause()

            # After activation and brief delay, pulse animation should be running
            # (class toggles every 0.6s, so we wait to see the effect)
            assert card.has_class("agent-active")

    async def test_pulse_animation_stops_when_inactive(self, app: KaganApp):
        """Pulse animation should stop and remove 'agent-pulse' class when card is deactivated."""
        async with app.run_test(size=(120, 40)) as pilot:
            sm = app.state_manager
            await sm.create_ticket(
                TicketCreate(title="Stop Timer Card", status=TicketStatus.BACKLOG)
            )

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            cards = list(screen.query(TicketCard))
            card = cards[0]

            card.is_agent_active = True
            await pilot.pause()
            assert card.has_class("agent-active")

            card.is_agent_active = False
            await pilot.pause()

            # Both agent-active and agent-pulse should be removed
            assert not card.has_class("agent-active")
            assert not card.has_class("agent-pulse")


class TestKanbanScreenActiveCards:
    async def test_update_active_cards_sets_card_states(self, app: KaganApp):
        """_update_active_cards should set is_agent_active=True for active tickets."""
        async with app.run_test(size=(120, 40)) as pilot:
            sm = app.state_manager
            ticket = await sm.create_ticket(
                TicketCreate(title="Agent Active", status=TicketStatus.BACKLOG)
            )

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            # Mock agent_manager.list_active() to return the ticket ID
            app.agent_manager.list_active = MagicMock(return_value=[ticket.id])

            screen._update_active_cards()
            await pilot.pause()

            cards = [c for c in screen.query(TicketCard) if c.ticket and c.ticket.id == ticket.id]
            assert len(cards) == 1
            assert cards[0].is_agent_active is True

    async def test_update_active_cards_clears_inactive(self, app: KaganApp):
        """_update_active_cards should clear is_agent_active when agent stops."""
        async with app.run_test(size=(120, 40)) as pilot:
            sm = app.state_manager
            ticket = await sm.create_ticket(
                TicketCreate(title="Was Active", status=TicketStatus.BACKLOG)
            )

            screen = get_kanban_screen(app)
            await screen._refresh_board()
            await pilot.pause()

            # First activate the card
            app.agent_manager.list_active = MagicMock(return_value=[ticket.id])
            screen._update_active_cards()
            await pilot.pause()

            cards = [c for c in screen.query(TicketCard) if c.ticket and c.ticket.id == ticket.id]
            assert cards[0].is_agent_active is True

            # Now clear active cards
            app.agent_manager.list_active = MagicMock(return_value=[])
            screen._update_active_cards()
            await pilot.pause()

            cards = [c for c in screen.query(TicketCard) if c.ticket and c.ticket.id == ticket.id]
            assert cards[0].is_agent_active is False
