"""Integration tests for Kanban flow."""

from __future__ import annotations

from collections.abc import AsyncGenerator  # noqa: TC003
from pathlib import Path  # noqa: TC003
from unittest.mock import AsyncMock

import pytest

from kagan.app import KaganApp
from kagan.config import KaganConfig
from kagan.database.manager import StateManager
from kagan.database.models import TicketCreate, TicketStatus
from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.widgets.card import TicketCard


class MockWorktreeManager:
    def __init__(self, worktree_path: Path) -> None:
        self.get_path = AsyncMock(return_value=None)
        self.create = AsyncMock(return_value=worktree_path)


class MockSessionManager:
    def __init__(self) -> None:
        self.session_exists = AsyncMock(return_value=False)
        self.create_session = AsyncMock(return_value="kagan-test-session")
        self.attach_session = AsyncMock()


@pytest.fixture
async def app_with_mocks(tmp_path: Path) -> AsyncGenerator[KaganApp, None]:
    """Create app with mocked worktree and session managers."""
    config_dir = tmp_path / ".kagan"
    config_dir.mkdir()
    config_file = config_dir / "config.toml"
    config_file.write_text("[general]\n")

    app = KaganApp(db_path=str(config_dir / "state.db"), config_path=str(config_file))
    app.config = KaganConfig()
    app._state_manager = StateManager(str(config_dir / "state.db"))
    await app._state_manager.initialize()
    app._worktree_manager = MockWorktreeManager(tmp_path / "worktree")  # type: ignore[assignment]
    app._session_manager = MockSessionManager()  # type: ignore[assignment]
    yield app
    await app._state_manager.close()


class TestKanbanFlow:
    async def test_open_session_moves_ticket_to_in_progress(self, app_with_mocks: KaganApp):
        """Test that opening a session moves ticket from BACKLOG to IN_PROGRESS."""
        ticket = await app_with_mocks.state_manager.create_ticket(
            TicketCreate(title="Feature", status=TicketStatus.BACKLOG)
        )

        async with app_with_mocks.run_test(size=(120, 40)) as pilot:
            await app_with_mocks.push_screen(KanbanScreen())
            await pilot.pause(0.3)

            screen = app_with_mocks.screen
            assert isinstance(screen, KanbanScreen)

            cards = [card for card in screen.query(TicketCard) if card.ticket]
            assert len(cards) > 0, "No cards found"
            cards[0].focus()
            await pilot.pause()

            await screen.action_open_session()
            await pilot.pause(0.3)

            updated = await app_with_mocks.state_manager.get_ticket(ticket.id)
            assert updated is not None
            assert updated.status == TicketStatus.IN_PROGRESS

            worktree = app_with_mocks.worktree_manager
            assert isinstance(worktree.create, AsyncMock)
            worktree.create.assert_awaited()
