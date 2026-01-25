"""Tests for ChatScreen."""

from __future__ import annotations

import pytest

from kagan.app import KaganApp
from kagan.ui.screens.chat import PLANNER_SESSION_ID, ChatScreen
from kagan.ui.screens.kanban import KanbanScreen


@pytest.fixture
def app():
    return KaganApp(db_path=":memory:")


class TestChatScreen:
    """Tests for ChatScreen."""

    async def test_chat_screen_composes(self, app: KaganApp):
        """Test ChatScreen composes correctly."""
        async with app.run_test(size=(120, 40)) as pilot:
            await pilot.press("c")
            await pilot.pause()

            screen = app.screen
            assert isinstance(screen, ChatScreen)
            assert screen.query_one("#chat-log")
            assert screen.query_one("#chat-input")
            assert screen.query_one("#chat-status")

    async def test_escape_returns_to_kanban(self, app: KaganApp):
        """Test escape returns to Kanban."""
        async with app.run_test(size=(120, 40)) as pilot:
            await pilot.press("c")
            await pilot.pause()
            await pilot.press("escape")
            await pilot.pause()

            assert isinstance(app.screen, KanbanScreen)

    async def test_planner_session_id_constant(self):
        """Test planner session ID is defined."""
        assert PLANNER_SESSION_ID == "planner"
