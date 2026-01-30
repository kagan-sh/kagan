"""E2E tests for PlannerScreen with mock ACP agent.

Tests: compose, navigation, input submission, plan response handling.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

if TYPE_CHECKING:
    from collections.abc import AsyncGenerator
    from pathlib import Path

from kagan.app import KaganApp
from kagan.database.manager import StateManager
from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.screens.planner import PlannerScreen

pytestmark = pytest.mark.e2e


class FakeAgent:
    """Fake ACP agent for PlannerScreen tests."""

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
    """Create app with mock planner agent and state manager."""
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


class TestPlannerScreen:
    """Tests for PlannerScreen UI behavior."""

    async def test_planner_screen_composes(self, app_with_mock_planner: KaganApp):
        """Test PlannerScreen composes correctly."""
        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(PlannerScreen())
            await pilot.pause()

            screen = app_with_mock_planner.screen
            assert isinstance(screen, PlannerScreen)
            assert screen.query_one("#planner-output")
            assert screen.query_one("#planner-input")
            assert screen.query_one("#planner-header")

    async def test_escape_navigates_to_board(self, app_with_mock_planner: KaganApp):
        """Test escape navigates to Kanban board."""
        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(PlannerScreen())
            await pilot.pause()
            await pilot.press("escape")
            await pilot.pause()

            assert isinstance(app_with_mock_planner.screen, KanbanScreen)

    async def test_input_submission_triggers_planner(self, app_with_mock_planner: KaganApp):
        """Test submitting input triggers planner agent."""
        from kagan.acp import messages
        from kagan.ui.screens.planner import PlannerInput

        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(PlannerScreen())
            await pilot.pause()

            screen = app_with_mock_planner.screen
            assert isinstance(screen, PlannerScreen)
            await screen.on_agent_ready(messages.AgentReady())
            await pilot.pause()

            planner_input = screen.query_one("#planner-input", PlannerInput)
            planner_input.focus()
            planner_input.insert("Add user authentication")
            await pilot.pause()

            await pilot.press("enter")
            await pilot.pause(0.5)

            agent = screen._agent
            assert agent is not None
            assert getattr(agent, "sent_prompts", [])

    async def test_plan_response_shows_approval_screen(self, app_with_mock_planner: KaganApp):
        """Test that planner response with <plan> shows approval screen."""
        from kagan.ui.screens.approval import ApprovalScreen

        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(PlannerScreen())
            await pilot.pause()

            screen = app_with_mock_planner.screen
            assert isinstance(screen, PlannerScreen)

            screen._accumulated_response = [
                """<plan>
<ticket>
<title>Add login feature</title>
<type>PAIR</type>
<description>Implement OAuth login</description>
<priority>high</priority>
</ticket>
</plan>"""
            ]

            await screen._try_create_tickets()
            await pilot.pause()

            assert isinstance(app_with_mock_planner.screen, ApprovalScreen)

    async def test_no_plan_block_continues_conversation(self, app_with_mock_planner: KaganApp):
        """Test that response without <plan> continues conversation."""
        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(PlannerScreen())
            await pilot.pause()

            screen = app_with_mock_planner.screen
            assert isinstance(screen, PlannerScreen)

            screen._accumulated_response = [
                "What features do you need? Should I include user authentication?"
            ]

            await screen._try_create_tickets()
            await pilot.pause()

            assert isinstance(app_with_mock_planner.screen, PlannerScreen)

    async def test_textarea_auto_height_expands(self, app_with_mock_planner: KaganApp):
        """Test PlannerInput expands height with multiline content."""
        from kagan.acp import messages
        from kagan.ui.screens.planner import PlannerInput

        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(PlannerScreen())
            await pilot.pause()

            screen = app_with_mock_planner.screen
            assert isinstance(screen, PlannerScreen)
            await screen.on_agent_ready(messages.AgentReady())
            await pilot.pause()

            planner_input = screen.query_one("#planner-input", PlannerInput)
            planner_input.insert("Line 1\nLine 2\nLine 3\nLine 4")
            await pilot.pause()

            assert planner_input.size.height >= 2

    async def test_textarea_disabled_on_init(self, app_with_mock_planner: KaganApp):
        """Test PlannerInput is read-only before agent ready."""
        from kagan.ui.screens.planner import PlannerInput

        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(PlannerScreen())
            await pilot.pause()

            screen = app_with_mock_planner.screen
            assert isinstance(screen, PlannerScreen)

            planner_input = screen.query_one("#planner-input", PlannerInput)
            assert planner_input.read_only is True
            assert planner_input.has_class("-disabled")
