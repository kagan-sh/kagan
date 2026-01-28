"""Tests for PlannerScreen with mock ACP agent."""

from __future__ import annotations

from collections.abc import AsyncGenerator  # noqa: TC003
from pathlib import Path  # noqa: TC003

import pytest

from kagan.agents.planner import build_planner_prompt, parse_plan
from kagan.app import KaganApp
from kagan.database.manager import StateManager
from kagan.database.models import TicketPriority, TicketType
from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.screens.planner import PlannerScreen


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
async def app_with_mock_planner(monkeypatch, tmp_path) -> AsyncGenerator[KaganApp, None]:
    """Create app with mock planner agent and state manager."""
    monkeypatch.setattr("kagan.ui.screens.planner.Agent", FakeAgent)
    app = KaganApp(db_path=":memory:")
    app._state_manager = StateManager(":memory:")
    await app._state_manager.initialize()

    # Create a minimal config
    from kagan.agents.scheduler import Scheduler
    from kagan.agents.worktree import WorktreeManager
    from kagan.config import KaganConfig
    from kagan.sessions.manager import SessionManager

    app.config = KaganConfig()

    # Initialize required managers for KanbanScreen
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

    async def test_input_submission_triggers_planner(self, app_with_mock_planner: KaganApp):
        """Test submitting input triggers planner agent."""
        from kagan.acp import messages

        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(PlannerScreen())
            await pilot.pause()

            # Trigger agent ready to enable input
            screen = app_with_mock_planner.screen
            assert isinstance(screen, PlannerScreen)
            await screen.on_agent_ready(messages.AgentReady())
            await pilot.pause()

            # Type and submit input
            for char in "Add user authentication":
                await pilot.press(char)
            await pilot.press("enter")
            await pilot.pause(0.5)  # Wait for async processing

            # Verify agent was spawned and prompt sent
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

            # Simulate agent response with plan
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

            await screen._try_create_ticket_from_response()
            await pilot.pause()

            # Should show ApprovalScreen
            assert isinstance(app_with_mock_planner.screen, ApprovalScreen)

    async def test_no_plan_block_continues_conversation(self, app_with_mock_planner: KaganApp):
        """Test that response without <plan> continues conversation."""
        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(PlannerScreen())
            await pilot.pause()

            screen = app_with_mock_planner.screen
            assert isinstance(screen, PlannerScreen)

            # Simulate agent asking questions (no plan block)
            screen._accumulated_response = [
                "What features do you need? Should I include user authentication?"
            ]

            await screen._try_create_ticket_from_response()
            await pilot.pause()

            # Should stay on PlannerScreen (no approval shown)
            assert isinstance(app_with_mock_planner.screen, PlannerScreen)


class TestParsePlan:
    """Test parse_plan function for multi-ticket parsing."""

    def test_parse_single_ticket_in_plan(self) -> None:
        """Parse a plan with a single ticket."""
        response = """
        Here's my plan:
        <plan>
        <ticket>
          <title>Add user authentication</title>
          <type>PAIR</type>
          <description>Implement login/logout functionality</description>
          <acceptance_criteria>
            <criterion>Users can log in</criterion>
            <criterion>Users can log out</criterion>
          </acceptance_criteria>
          <priority>high</priority>
        </ticket>
        </plan>
        """
        tickets = parse_plan(response)
        assert len(tickets) == 1
        assert tickets[0].title == "Add user authentication"
        assert tickets[0].ticket_type == TicketType.PAIR
        assert tickets[0].priority == TicketPriority.HIGH
        assert len(tickets[0].acceptance_criteria) == 2


class TestBuildPlannerPrompt:
    """Test build_planner_prompt always includes format instructions."""

    def test_format_instructions_always_included(self) -> None:
        """Format instructions (AUTO/PAIR, XML format) must always be present."""
        prompt = build_planner_prompt("Create a login feature")

        # Must include XML format instructions
        assert "<plan>" in prompt
        assert "<ticket>" in prompt
        assert "<type>AUTO or PAIR</type>" in prompt

        # Must include AUTO/PAIR guidance
        assert "**AUTO**" in prompt
        assert "**PAIR**" in prompt
        assert "Bug fixes with clear steps" in prompt
        assert "New feature design" in prompt

    def test_user_request_included(self) -> None:
        """User request should be included in the final prompt."""
        prompt = build_planner_prompt("Implement OAuth login with Google")

        assert "Implement OAuth login with Google" in prompt
        assert "## User Request" in prompt

    def test_parse_multiple_tickets(self) -> None:
        """Parse a plan with multiple tickets."""
        response = """
        <plan>
        <ticket>
          <title>Create database schema</title>
          <type>AUTO</type>
          <description>Set up initial database tables</description>
          <priority>high</priority>
        </ticket>
        <ticket>
          <title>Build REST API</title>
          <type>PAIR</type>
          <description>Create API endpoints</description>
          <priority>medium</priority>
        </ticket>
        <ticket>
          <title>Add logging</title>
          <type>AUTO</type>
          <description>Implement logging infrastructure</description>
          <priority>low</priority>
        </ticket>
        </plan>
        """
        tickets = parse_plan(response)
        assert len(tickets) == 3
        assert tickets[0].title == "Create database schema"
        assert tickets[0].ticket_type == TicketType.AUTO
        assert tickets[1].title == "Build REST API"
        assert tickets[1].ticket_type == TicketType.PAIR
        assert tickets[2].title == "Add logging"
        assert tickets[2].ticket_type == TicketType.AUTO

    def test_parse_plan_default_type_is_pair(self) -> None:
        """Default ticket type should be PAIR when not specified."""
        response = """
        <plan>
        <ticket>
          <title>Design new feature</title>
          <description>Feature without type specified</description>
        </ticket>
        </plan>
        """
        tickets = parse_plan(response)
        assert len(tickets) == 1
        assert tickets[0].ticket_type == TicketType.PAIR

    def test_parse_plan_no_plan_block(self) -> None:
        """Return empty list when no plan block found."""
        response = "I need more information. What features do you need?"
        tickets = parse_plan(response)
        assert tickets == []

    def test_parse_plan_malformed_xml(self) -> None:
        """Handle malformed XML gracefully."""
        response = """
        <plan>
        <ticket>
          <title>Broken ticket
          <description>Missing closing tags
        </ticket>
        </plan>
        """
        tickets = parse_plan(response)
        assert tickets == []

    def test_parse_plan_empty_plan(self) -> None:
        """Handle empty plan block."""
        response = "<plan></plan>"
        tickets = parse_plan(response)
        assert tickets == []

    def test_parse_plan_case_insensitive(self) -> None:
        """Plan wrapper tags should be case insensitive (inner tags are case-sensitive)."""
        response = """
        <PLAN>
        <ticket>
          <title>Test ticket</title>
          <type>auto</type>
          <description>Testing case insensitivity</description>
        </ticket>
        </PLAN>
        """
        tickets = parse_plan(response)
        assert len(tickets) == 1
        assert tickets[0].title == "Test ticket"
        assert tickets[0].ticket_type == TicketType.AUTO

    def test_parse_plan_with_surrounding_text(self) -> None:
        """Parse plan when surrounded by other text."""
        response = """
        Based on your requirements, I've created a plan:

        <plan>
        <ticket>
          <title>Implement feature X</title>
          <type>PAIR</type>
          <description>Build the feature</description>
        </ticket>
        </plan>

        Let me know if you'd like any changes!
        """
        tickets = parse_plan(response)
        assert len(tickets) == 1
        assert tickets[0].title == "Implement feature X"


class TestApprovalScreen:
    """Test ApprovalScreen user-facing behavior."""

    async def test_approval_screen_displays_tickets(self, app_with_mock_planner: KaganApp):
        """Approval screen should display proposed tickets in a table."""
        from textual.widgets import DataTable

        from kagan.database.models import TicketCreate
        from kagan.ui.screens.approval import ApprovalScreen

        tickets = [
            TicketCreate(title="Task 1", description="Desc 1", ticket_type=TicketType.AUTO),
            TicketCreate(title="Task 2", description="Desc 2", ticket_type=TicketType.PAIR),
        ]

        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            await app_with_mock_planner.push_screen(ApprovalScreen(tickets))
            await pilot.pause()

            # Check DataTable exists and has rows
            table = app_with_mock_planner.screen.query_one(DataTable)
            assert table.row_count == 2

    async def test_approval_screen_toggle_type(self, app_with_mock_planner: KaganApp):
        """Pressing 't' should toggle ticket type."""
        from textual.widgets import DataTable

        from kagan.database.models import TicketCreate
        from kagan.ui.screens.approval import ApprovalScreen

        tickets = [
            TicketCreate(title="Task 1", description="Desc 1", ticket_type=TicketType.AUTO),
        ]

        async with app_with_mock_planner.run_test(size=(120, 40)) as pilot:
            screen = ApprovalScreen(tickets)
            await app_with_mock_planner.push_screen(screen)
            await pilot.pause()

            # Focus the table and toggle type
            table = app_with_mock_planner.screen.query_one(DataTable)
            table.focus()
            await pilot.pause()

            original_type = screen._tickets[0].ticket_type
            await pilot.press("t")
            await pilot.pause()

            # Type should be toggled
            new_type = screen._tickets[0].ticket_type
            assert new_type != original_type

    async def test_approval_screen_escape_cancels(self, app_with_mock_planner: KaganApp):
        """Pressing escape should dismiss the approval screen."""
        from kagan.database.models import TicketCreate
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

            # Result should be None (cancelled)
            assert result_holder["result"] is None

    async def test_approval_screen_approve_button(self, app_with_mock_planner: KaganApp):
        """Clicking approve button should approve tickets."""
        from textual.widgets import Button

        from kagan.database.models import TicketCreate
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

            # Click the approve button
            approve_btn = app_with_mock_planner.screen.query_one("#approve", Button)
            approve_btn.press()
            await pilot.pause()

            # Result should be the list of tickets
            assert isinstance(result_holder["result"], list)
            assert len(result_holder["result"]) == 1
