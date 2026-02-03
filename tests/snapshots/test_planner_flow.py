"""Snapshot tests for Planner screen user flows.

These tests cover the main planner interaction flows:
- Empty state
- User input
- Plan proposal from agent
- Plan approval/dismissal/refinement
- Multi-ticket plans
- Clarification requests

Note: Tests are synchronous because pytest-textual-snapshot's snap_compare
internally calls asyncio.run(), which conflicts with async test functions.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import pytest

from tests.snapshots.helpers import (
    type_text,
    wait_for_planner_ready,
    wait_for_widget,
    wait_for_workers,
)
from tests.snapshots.mock_responses import (
    MULTI_TICKET_PLAN_TOOL_CALLS,
    PLAN_PROPOSAL_RESPONSE,
    PLAN_PROPOSAL_TOOL_CALLS,
    TASK_NEEDS_CLARIFICATION_RESPONSE,
)

if TYPE_CHECKING:
    from types import SimpleNamespace

    from textual.pilot import Pilot

    from tests.snapshots.conftest import MockAgentFactory


def _create_fake_tmux(sessions: dict[str, Any]) -> object:
    """Create a fake tmux function that tracks session state."""

    async def fake_run_tmux(*args: str) -> str:
        if not args:
            return ""
        command, args_list = args[0], list(args)
        if command == "new-session" and "-s" in args_list:
            idx = args_list.index("-s")
            name = args_list[idx + 1] if idx + 1 < len(args_list) else None
            if name:
                cwd = args_list[args_list.index("-c") + 1] if "-c" in args_list else ""
                env: dict[str, str] = {}
                for i, val in enumerate(args_list):
                    if val == "-e" and i + 1 < len(args_list):
                        key, _, env_value = args_list[i + 1].partition("=")
                        env[key] = env_value
                sessions[name] = {"cwd": cwd, "env": env, "sent_keys": []}
        elif command == "kill-session" and "-t" in args_list:
            sessions.pop(args_list[args_list.index("-t") + 1], None)
        elif command == "send-keys" and "-t" in args_list:
            idx = args_list.index("-t")
            name = args_list[idx + 1]
            keys = args_list[idx + 2] if idx + 2 < len(args_list) else ""
            if name in sessions:
                sessions[name]["sent_keys"].append(keys)
        elif command == "list-sessions":
            return "\n".join(sorted(sessions.keys()))
        return ""

    return fake_run_tmux


class TestPlannerFlow:
    """Snapshot tests for Planner screen user flows."""

    @pytest.mark.snapshot
    def test_planner_empty_state(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snapshot_terminal_size: tuple[int, int],
        snap_compare,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """Test fresh app shows planner with empty chat."""
        from kagan.app import KaganApp

        # Mock tmux
        sessions: dict[str, Any] = {}
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", _create_fake_tmux(sessions))
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", _create_fake_tmux(sessions))

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Verify we're on PlannerScreen (empty board = planner first)
            from kagan.ui.screens.planner import PlannerScreen

            assert isinstance(pilot.app.screen, PlannerScreen)

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_planner_user_input(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snapshot_terminal_size: tuple[int, int],
        snap_compare,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """Test user types a prompt, shows in input area."""
        from kagan.app import KaganApp

        # Mock tmux
        sessions: dict[str, Any] = {}
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", _create_fake_tmux(sessions))
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", _create_fake_tmux(sessions))

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Type a prompt into the input
            await type_text(pilot, "Add user authentication to the API")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_planner_plan_proposal(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snapshot_terminal_size: tuple[int, int],
        snap_compare,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """Test agent returns plan proposal with tickets.

        After user sends prompt, agent returns a plan proposal.
        Screen should show plan approval widget with ticket list.
        """
        from kagan.app import KaganApp

        # Configure mock agent with plan proposal response
        mock_acp_agent_factory.set_default_response(PLAN_PROPOSAL_RESPONSE)
        mock_acp_agent_factory.set_default_tool_calls(PLAN_PROPOSAL_TOOL_CALLS)

        # Mock tmux
        sessions: dict[str, Any] = {}
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", _create_fake_tmux(sessions))
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", _create_fake_tmux(sessions))

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Wait for agent to be ready before typing
            await wait_for_planner_ready(pilot)
            # Type and submit a prompt
            await type_text(pilot, "Add user authentication")
            await pilot.pause()
            await pilot.press("enter")
            # Wait for workers to complete (the agent response worker)
            await wait_for_workers(pilot, timeout=10.0)
            # Wait for plan approval widget to appear
            await wait_for_widget(pilot, "PlanApprovalWidget", timeout=10.0)
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_planner_plan_accept(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snapshot_terminal_size: tuple[int, int],
        snap_compare,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """Test user accepts plan (presses 'a').

        Plan approval should be acknowledged and tickets created.
        """
        from kagan.app import KaganApp

        # Configure mock agent with plan proposal response
        mock_acp_agent_factory.set_default_response(PLAN_PROPOSAL_RESPONSE)
        mock_acp_agent_factory.set_default_tool_calls(PLAN_PROPOSAL_TOOL_CALLS)

        # Mock tmux
        sessions: dict[str, Any] = {}
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", _create_fake_tmux(sessions))
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", _create_fake_tmux(sessions))

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Wait for agent to be ready before typing
            await wait_for_planner_ready(pilot)
            # Type and submit a prompt
            await type_text(pilot, "Add user authentication")
            await pilot.pause()
            await pilot.press("enter")
            # Wait for workers to complete (the agent response worker)
            await wait_for_workers(pilot, timeout=10.0)
            # Wait for plan approval widget
            await wait_for_widget(pilot, "PlanApprovalWidget", timeout=10.0)
            await pilot.pause()
            # Press 'a' to approve the plan
            await pilot.press("a")
            # Give time for tickets to be created and UI to update
            await pilot.pause()
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_planner_plan_decline(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snapshot_terminal_size: tuple[int, int],
        snap_compare,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """Test user declines plan (presses 'd').

        Shows decline feedback and prompts for what to change.
        """
        from kagan.app import KaganApp

        # Configure mock agent with plan proposal response
        mock_acp_agent_factory.set_default_response(PLAN_PROPOSAL_RESPONSE)
        mock_acp_agent_factory.set_default_tool_calls(PLAN_PROPOSAL_TOOL_CALLS)

        # Mock tmux
        sessions: dict[str, Any] = {}
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", _create_fake_tmux(sessions))
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", _create_fake_tmux(sessions))

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Wait for agent to be ready before typing
            await wait_for_planner_ready(pilot)
            # Type and submit a prompt
            await type_text(pilot, "Add user authentication")
            await pilot.pause()
            await pilot.press("enter")
            # Wait for workers to complete (the agent response worker)
            await wait_for_workers(pilot, timeout=10.0)
            # Wait for plan approval widget
            await wait_for_widget(pilot, "PlanApprovalWidget", timeout=10.0)
            await pilot.pause()
            # Press 'd' to dismiss the plan
            await pilot.press("d")
            # Give time for dismiss feedback
            await pilot.pause()
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_planner_plan_refine(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snapshot_terminal_size: tuple[int, int],
        snap_compare,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """Test user requests refinement (presses 'e' to edit).

        Shows the ticket editor screen for refinement.
        """
        from kagan.app import KaganApp

        # Configure mock agent with plan proposal response
        mock_acp_agent_factory.set_default_response(PLAN_PROPOSAL_RESPONSE)
        mock_acp_agent_factory.set_default_tool_calls(PLAN_PROPOSAL_TOOL_CALLS)

        # Mock tmux
        sessions: dict[str, Any] = {}
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", _create_fake_tmux(sessions))
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", _create_fake_tmux(sessions))

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Wait for agent to be ready before typing
            await wait_for_planner_ready(pilot)
            # Type and submit a prompt
            await type_text(pilot, "Add user authentication")
            await pilot.pause()
            await pilot.press("enter")
            # Wait for workers to complete (the agent response worker)
            await wait_for_workers(pilot, timeout=10.0)
            # Wait for plan approval widget
            await wait_for_widget(pilot, "PlanApprovalWidget", timeout=10.0)
            await pilot.pause()
            # Press 'e' to edit/refine the plan
            await pilot.press("e")
            # Give time for editor screen to appear
            await pilot.pause()
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_planner_multi_ticket_plan(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snapshot_terminal_size: tuple[int, int],
        snap_compare,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """Test agent proposes multiple tickets.

        Shows list with multiple items in plan approval widget.
        """
        from kagan.app import KaganApp

        # Configure mock agent with multi-ticket plan response
        mock_acp_agent_factory.set_default_response(PLAN_PROPOSAL_RESPONSE)
        mock_acp_agent_factory.set_default_tool_calls(MULTI_TICKET_PLAN_TOOL_CALLS)

        # Mock tmux
        sessions: dict[str, Any] = {}
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", _create_fake_tmux(sessions))
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", _create_fake_tmux(sessions))

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            from kagan.ui.screens.planner import PlannerScreen

            assert isinstance(pilot.app.screen, PlannerScreen)
            # Wait for agent to be ready before typing
            await wait_for_planner_ready(pilot)
            # Type and submit a prompt
            await type_text(pilot, "Build complete user system with auth and profiles")
            await pilot.pause()
            await pilot.press("enter")
            # Wait for workers to complete (the agent response worker)
            await wait_for_workers(pilot, timeout=10.0)
            # Wait for plan approval widget
            await wait_for_widget(pilot, "PlanApprovalWidget", timeout=10.0)
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_planner_clarification_request(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snapshot_terminal_size: tuple[int, int],
        snap_compare,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """Test agent asks for clarification.

        Shows clarification prompt instead of plan approval.
        """
        from kagan.app import KaganApp

        # Configure mock agent with clarification response (no tool calls)
        mock_acp_agent_factory.set_default_response(TASK_NEEDS_CLARIFICATION_RESPONSE)
        mock_acp_agent_factory.set_default_tool_calls({})  # No plan proposed

        # Mock tmux
        sessions: dict[str, Any] = {}
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", _create_fake_tmux(sessions))
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", _create_fake_tmux(sessions))

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            from kagan.ui.screens.planner import PlannerScreen

            assert isinstance(pilot.app.screen, PlannerScreen)
            # Wait for agent to be ready before typing
            await wait_for_planner_ready(pilot)
            # Type and submit an ambiguous prompt
            await type_text(pilot, "Improve the performance")
            await pilot.pause()
            await pilot.press("enter")
            # Wait for workers to complete (the agent response worker)
            await wait_for_workers(pilot, timeout=10.0)
            # Wait for user input to appear (shows the submitted prompt)
            await wait_for_widget(pilot, "UserInput", timeout=10.0)
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)

    @pytest.mark.snapshot
    def test_planner_navigate_ticket_list(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snapshot_terminal_size: tuple[int, int],
        snap_compare,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """Test navigating through tickets in plan approval widget.

        User can use up/down to select different tickets.
        """
        from kagan.app import KaganApp

        # Configure mock agent with multi-ticket plan response
        mock_acp_agent_factory.set_default_response(PLAN_PROPOSAL_RESPONSE)
        mock_acp_agent_factory.set_default_tool_calls(MULTI_TICKET_PLAN_TOOL_CALLS)

        # Mock tmux
        sessions: dict[str, Any] = {}
        monkeypatch.setattr("kagan.sessions.tmux.run_tmux", _create_fake_tmux(sessions))
        monkeypatch.setattr("kagan.sessions.manager.run_tmux", _create_fake_tmux(sessions))

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            # Wait for agent to be ready before typing
            await wait_for_planner_ready(pilot)
            # Type and submit a prompt
            await type_text(pilot, "Build user system")
            await pilot.pause()
            await pilot.press("enter")
            # Wait for workers to complete (the agent response worker)
            await wait_for_workers(pilot, timeout=10.0)
            # Wait for plan approval widget
            await wait_for_widget(pilot, "PlanApprovalWidget", timeout=10.0)
            await pilot.pause()
            # Navigate down through tickets
            await pilot.press("down")
            await pilot.press("down")
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)
