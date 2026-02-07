"""Snapshot tests for Planner screen user flows.

These tests cover the main planner interaction flows:
- Empty state
- Plan proposal from agent
- Plan approval

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
from tests.snapshots.mock_responses import PLAN_PROPOSAL_RESPONSE, PLAN_PROPOSAL_TOOL_CALLS

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
        monkeypatch.setattr("kagan.tmux.run_tmux", _create_fake_tmux(sessions))
        monkeypatch.setattr("kagan.services.sessions.run_tmux", _create_fake_tmux(sessions))

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            project_root=snapshot_project.root,
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
    def test_planner_plan_proposal(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snapshot_terminal_size: tuple[int, int],
        snap_compare,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """Test agent returns plan proposal with tasks.

        After user sends prompt, agent returns a plan proposal.
        Screen should show plan approval widget with task list.
        """
        from kagan.app import KaganApp

        # Configure mock agent with plan proposal response
        mock_acp_agent_factory.set_default_response(PLAN_PROPOSAL_RESPONSE)
        mock_acp_agent_factory.set_default_tool_calls(PLAN_PROPOSAL_TOOL_CALLS)

        # Mock tmux
        sessions: dict[str, Any] = {}
        monkeypatch.setattr("kagan.tmux.run_tmux", _create_fake_tmux(sessions))
        monkeypatch.setattr("kagan.services.sessions.run_tmux", _create_fake_tmux(sessions))

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            project_root=snapshot_project.root,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()
            from kagan.ui.screens.planner import PlannerInput
            from kagan.ui.widgets.plan_approval import PlanApprovalWidget

            # Wait for agent to be ready before typing
            await wait_for_planner_ready(pilot)
            # Type and submit a prompt
            await type_text(pilot, "Add user authentication")
            await pilot.pause()
            await pilot.press("enter")
            # Wait for workers to complete (the agent response worker)
            await wait_for_workers(pilot, timeout=20.0)
            # Wait for plan approval widget to appear
            await wait_for_widget(pilot, "PlanApprovalWidget", timeout=20.0)
            plan_widget = pilot.app.screen.query_one(PlanApprovalWidget)
            plan_widget.focus()
            planner_input = pilot.app.screen.query_one("#planner-input", PlannerInput)
            planner_input.blur()
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

        Plan approval should be acknowledged and tasks created.
        """
        from kagan.app import KaganApp

        # Configure mock agent with plan proposal response
        mock_acp_agent_factory.set_default_response(PLAN_PROPOSAL_RESPONSE)
        mock_acp_agent_factory.set_default_tool_calls(PLAN_PROPOSAL_TOOL_CALLS)

        # Mock tmux
        sessions: dict[str, Any] = {}
        monkeypatch.setattr("kagan.tmux.run_tmux", _create_fake_tmux(sessions))
        monkeypatch.setattr("kagan.services.sessions.run_tmux", _create_fake_tmux(sessions))

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            lock_path=None,
            project_root=snapshot_project.root,
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
            await wait_for_workers(pilot, timeout=20.0)
            # Wait for plan approval widget
            await wait_for_widget(pilot, "PlanApprovalWidget", timeout=20.0)
            await pilot.pause()
            # Press 'a' to approve the plan
            await pilot.press("a")
            # Give time for tasks to be created and UI to update
            await pilot.pause()
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)
