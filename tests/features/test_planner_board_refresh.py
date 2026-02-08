from __future__ import annotations

from typing import cast

import pytest
from tests.helpers.mock_responses import make_propose_plan_tool_call
from tests.helpers.wait import type_text, wait_for_planner_ready, wait_for_screen, wait_for_widget
from textual.widgets import Input

from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.screens.planner import PlannerScreen
from kagan.ui.widgets.plan_approval import PlanApprovalWidget


@pytest.mark.asyncio
async def test_planner_approval_returns_to_fresh_board(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    created_title = "Planner created task: board refresh regression"
    mock_agent_factory.set_default_response("Plan ready.")
    mock_agent_factory.set_default_tool_calls(
        make_propose_plan_tool_call(
            tool_call_id="tc-board-refresh-001",
            tasks=[
                {
                    "title": created_title,
                    "type": "AUTO",
                    "description": "Ensure board is refreshed when returning from planner.",
                    "acceptance_criteria": ["Task is visible on Kanban after approve"],
                    "priority": "low",
                }
            ],
            todos=[
                {"content": "Build a plan", "status": "completed"},
                {"content": "Submit task proposal", "status": "completed"},
            ],
        )
    )

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))

        # Simulate stale board filter state before entering planner.
        kanban.action_toggle_search()
        await pilot.pause()
        kanban.query_one("#search-input", Input).value = "query-that-matches-nothing"
        await pilot.pause()

        kanban.action_open_planner()
        await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
        await wait_for_planner_ready(pilot, timeout=10.0)
        await type_text(pilot, "Create one task")
        await pilot.press("enter")
        await wait_for_widget(pilot, "PlanApprovalWidget", timeout=20.0)

        planner = cast("PlannerScreen", pilot.app.screen)
        plan_widget = planner.query_one(PlanApprovalWidget)
        plan_widget.focus()
        await pilot.pause()
        plan_widget.action_approve()

        await wait_for_screen(pilot, KanbanScreen, timeout=20.0)
        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        created_task = next(task for task in tasks if task.title == created_title)
        await wait_for_widget(pilot, f"#card-{created_task.id}", timeout=10.0)
