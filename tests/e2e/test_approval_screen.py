"""E2E tests for ApprovalScreen behavior.

Tests: ticket display, type toggle, escape/approve actions.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

if TYPE_CHECKING:
    from collections.abc import AsyncGenerator
    from pathlib import Path

from kagan.app import KaganApp
from kagan.database.manager import StateManager
from kagan.database.models import TicketCreate, TicketType

pytestmark = pytest.mark.e2e


class FakeAgent:
    """Fake ACP agent for tests."""

    def __init__(self, cwd: Path, agent_config: object, *, read_only: bool = False) -> None:
        self.started = False
        self.sent_prompts: list[str] = []
        self.read_only = read_only

    def start(self, message_target: object | None = None) -> None:
        self.started = True

    async def wait_ready(self, timeout: float = 30.0) -> None:
        return None

    async def send_prompt(self, prompt: str) -> str | None:
        self.sent_prompts.append(prompt)
        return "end_turn"

    async def stop(self) -> None:
        self.started = False

    def set_auto_approve(self, enabled: bool) -> None:
        """Mock set_auto_approve for tests."""
        pass


@pytest.fixture
async def app_with_mock_planner(monkeypatch, tmp_path) -> AsyncGenerator[KaganApp, None]:
    """Create app with mock planner agent."""
    monkeypatch.setattr("kagan.ui.screens.planner.Agent", FakeAgent)
    app = KaganApp(db_path=":memory:")
    app._state_manager = StateManager(":memory:")
    await app._state_manager.initialize()

    from kagan.agents.scheduler import Scheduler
    from kagan.agents.worktree import WorktreeManager
    from kagan.config import KaganConfig
    from kagan.sessions.manager import SessionManager

    app.config = KaganConfig()

    project_root = tmp_path / "test_project"
    project_root.mkdir(exist_ok=True)

    app._worktree_manager = WorktreeManager(repo_root=project_root)
    app._session_manager = SessionManager(
        project_root=project_root, state=app._state_manager, config=app.config
    )
    app._scheduler = Scheduler(
        state_manager=app._state_manager,
        worktree_manager=app._worktree_manager,
        config=app.config,
        session_manager=app._session_manager,
        on_ticket_changed=lambda: None,
        on_iteration_changed=lambda tid, it: None,
    )

    yield app
    await app._state_manager.close()


class TestApprovalScreen:
    """Test ApprovalScreen user-facing behavior."""

    async def test_approval_screen_displays_tickets(self, app_with_mock_planner: KaganApp):
        """Approval screen should display proposed tickets in a table."""
        from textual.widgets import DataTable

        from kagan.ui.screens.approval import ApprovalScreen

        tickets = [
            TicketCreate(title="Task 1", description="Desc 1", ticket_type=TicketType.AUTO),
            TicketCreate(title="Task 2", description="Desc 2", ticket_type=TicketType.PAIR),
        ]

        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(ApprovalScreen(tickets))
            await pilot.pause()

            table = app_with_mock_planner.screen.query_one(DataTable)
            assert table.row_count == 2

    async def test_approval_screen_toggle_type(self, app_with_mock_planner: KaganApp):
        """Pressing 't' should toggle ticket type."""
        from textual.widgets import DataTable

        from kagan.ui.screens.approval import ApprovalScreen

        tickets = [
            TicketCreate(title="Task 1", description="Desc 1", ticket_type=TicketType.AUTO),
        ]

        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            screen = ApprovalScreen(tickets)
            await app_with_mock_planner.push_screen(screen)
            await pilot.pause()

            table = app_with_mock_planner.screen.query_one(DataTable)
            table.focus()
            await pilot.pause()

            original_type = screen._tickets[0].ticket_type
            await pilot.press("t")
            await pilot.pause()

            new_type = screen._tickets[0].ticket_type
            assert new_type != original_type

    async def test_approval_screen_escape_cancels(self, app_with_mock_planner: KaganApp):
        """Pressing escape should dismiss the approval screen."""
        from kagan.ui.screens.approval import ApprovalScreen

        tickets = [
            TicketCreate(title="Task 1", description="Desc 1"),
        ]

        result_holder = {"result": "not_set"}

        def capture_result(result):
            result_holder["result"] = result

        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(ApprovalScreen(tickets), capture_result)
            await pilot.pause()

            await pilot.press("escape")
            await pilot.pause()

            assert result_holder["result"] is None

    async def test_approval_screen_approve_button(self, app_with_mock_planner: KaganApp):
        """Clicking approve button should approve tickets."""
        from textual.widgets import Button

        from kagan.ui.screens.approval import ApprovalScreen

        tickets = [
            TicketCreate(title="Task 1", description="Desc 1"),
        ]

        result_holder = {"result": "not_set"}

        def capture_result(result):
            result_holder["result"] = result

        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(ApprovalScreen(tickets), capture_result)
            await pilot.pause()

            approve_btn = app_with_mock_planner.screen.query_one("#approve", Button)
            approve_btn.press()
            await pilot.pause()

            assert isinstance(result_holder["result"], list)
            assert len(result_holder["result"]) == 1
