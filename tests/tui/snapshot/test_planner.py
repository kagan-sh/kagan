"""Snapshot tests for Planner screen user flows.

These tests cover the main planner interaction flows:
- Empty state
- Plan proposal from agent

Note: Tests are synchronous because pytest-textual-snapshot's snap_compare
internally calls asyncio.run(), which conflicts with async test functions.
"""

from __future__ import annotations

import asyncio
import sys
from typing import TYPE_CHECKING, Any

import pytest
from tests.helpers import wait_for_planner_ready, wait_for_screen, wait_for_widget
from tests.helpers.journey_runner import bundle_snapshots, execute_test_actions
from tests.helpers.mock_responses import PLAN_PROPOSAL_RESPONSE, PLAN_PROPOSAL_TOOL_CALLS
from tests.tui.snapshot.conftest import _normalize_svg

if TYPE_CHECKING:
    from types import SimpleNamespace

    from tests.tui.snapshot.conftest import MockAgentFactory

pytestmark = pytest.mark.usefixtures("global_mock_tmux")


class TestPlannerFlow:
    @pytest.mark.skipif(sys.platform == "win32", reason="Timing-sensitive; flaky on Windows CI")
    def test_planner_journey(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snapshot: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """Planner journey: empty state then plan proposal."""
        from kagan.tui.app import KaganApp

        mock_acp_agent_factory.set_default_response(PLAN_PROPOSAL_RESPONSE)
        mock_acp_agent_factory.set_default_tool_calls(PLAN_PROPOSAL_TOOL_CALLS)

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            project_root=snapshot_project.root,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_flow() -> dict[str, str]:
            from kagan.tui.ui.screens.planner import PlannerInput, PlannerScreen
            from kagan.tui.ui.widgets.plan_approval import PlanApprovalWidget

            cols, rows = snapshot_terminal_size
            async with app.run_test(headless=True, size=(cols, rows)) as pilot:
                await wait_for_screen(pilot, PlannerScreen, timeout=20.0)
                snapshots = await execute_test_actions(pilot, ["shot(empty)"])

                await wait_for_planner_ready(pilot, timeout=20.0)

                planner_input = pilot.app.screen.query_one("#planner-input", PlannerInput)
                planner_input.text = "Add user authentication"
                await pilot.pause()
                await pilot.press("enter")

                await wait_for_widget(pilot, "PlanApprovalWidget", timeout=20.0)
                plan_widget = pilot.app.screen.query_one(PlanApprovalWidget)
                plan_widget.focus()
                planner_input = pilot.app.screen.query_one("#planner-input", PlannerInput)
                planner_input.blur()
                await pilot.pause()
                snapshots.update(await execute_test_actions(pilot, ["shot(plan)"]))
                return snapshots

        snapshots = asyncio.run(run_flow())
        assert snapshots, "No snapshots captured for planner journey"
        bundle = bundle_snapshots(snapshots, normalizer=_normalize_svg)
        assert snapshot == bundle
