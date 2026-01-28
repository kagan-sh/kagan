"""Tests for PlannerScreen with mock ACP agent."""

from __future__ import annotations

from collections.abc import AsyncGenerator  # noqa: TC003
from pathlib import Path  # noqa: TC003

import pytest

from kagan.app import KaganApp
from kagan.database.manager import StateManager
from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.screens.planner import PLANNER_SESSION_ID, PlannerScreen


class FakeAgent:
    """Fake ACP agent for PlannerScreen tests."""

    def __init__(self, cwd: Path, agent_config: object) -> None:
        self.started = False
        self.sent_prompts: list[str] = []

    def start(self, message_target: object | None = None) -> None:
        self.started = True

    async def wait_ready(self, timeout: float = 30.0) -> None:
        return None

    async def send_prompt(self, prompt: str) -> str | None:
        self.sent_prompts.append(prompt)
        return "end_turn"

    async def stop(self) -> None:
        self.started = False


@pytest.fixture
async def app_with_mock_planner(monkeypatch) -> AsyncGenerator[KaganApp, None]:
    """Create app with mock planner agent and state manager."""
    monkeypatch.setattr("kagan.ui.screens.planner.Agent", FakeAgent)
    app = KaganApp(db_path=":memory:")
    app._state_manager = StateManager(":memory:")
    await app._state_manager.initialize()
    # Create a minimal config
    from kagan.config import KaganConfig

    app.config = KaganConfig()
    yield app
    await app._state_manager.close()


class TestPlannerScreen:
    """Tests for PlannerScreen."""

    async def test_planner_screen_composes(self, app_with_mock_planner: KaganApp):
        """Test PlannerScreen composes correctly."""
        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(PlannerScreen())
            await pilot.pause()

            screen = app_with_mock_planner.screen
            assert isinstance(screen, PlannerScreen)
            assert screen.query_one("#planner-output")  # StreamingOutput widget
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

    async def test_planner_session_id_constant(self):
        """Test planner session ID is defined."""
        assert PLANNER_SESSION_ID == "planner"

    async def test_input_submission_triggers_planner(self, app_with_mock_planner: KaganApp):
        """Test submitting input triggers planner agent."""
        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(PlannerScreen())
            await pilot.pause()

            # Type and submit input
            for char in "Add user authentication":
                await pilot.press(char)
            await pilot.press("enter")
            await pilot.pause(0.5)  # Wait for async processing

            # Verify agent was spawned and prompt sent
            screen = app_with_mock_planner.screen
            assert isinstance(screen, PlannerScreen)
            agent = screen._agent
            assert agent is not None
            assert getattr(agent, "sent_prompts", [])

    async def test_ticket_creation_from_planner_response(self, app_with_mock_planner: KaganApp):
        """Test that planner response creates a ticket."""
        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(PlannerScreen())
            await pilot.pause()

            screen = app_with_mock_planner.screen
            assert isinstance(screen, PlannerScreen)

            # Manually trigger ticket creation by simulating agent response
            screen._accumulated_response = [
                """<ticket>
<title>Add login feature</title>
<description>Implement OAuth login</description>
<priority>high</priority>
</ticket>"""
            ]

            await screen._try_create_ticket_from_response()
            await pilot.pause()

            # Verify ticket was created
            tickets = await app_with_mock_planner.state_manager.get_all_tickets()
            assert len(tickets) == 1
            assert tickets[0].title == "Add login feature"
            from kagan.database.models import TicketPriority

            assert tickets[0].priority == TicketPriority.HIGH

    async def test_planner_navigates_to_board_after_ticket_creation(
        self, app_with_mock_planner: KaganApp
    ):
        """Test that planner navigates to board after creating ticket."""
        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(PlannerScreen())
            await pilot.pause()

            screen = app_with_mock_planner.screen
            assert isinstance(screen, PlannerScreen)

            # Simulate successful ticket creation
            screen._accumulated_response = [
                """<ticket>
<title>Test ticket</title>
<description>Test description</description>
</ticket>"""
            ]

            await screen._try_create_ticket_from_response()
            await pilot.pause()

            # Should navigate to KanbanScreen
            assert isinstance(app_with_mock_planner.screen, KanbanScreen)
