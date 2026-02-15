from __future__ import annotations

from typing import cast

import pytest
from tests.helpers.wait import wait_for_screen

from kagan.tui.ui.modals.review import ReviewModal, extract_review_decision
from kagan.tui.ui.screens.kanban import KanbanScreen


@pytest.mark.asyncio
async def test_escape_closes_automation_managed_live_review_modal(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        tasks = await app.ctx.api.list_tasks(project_id=app.ctx.active_project_id)
        task = tasks[0]
        agent_config = task.get_agent_config(app.config)
        review_agent = mock_agent_factory(app.project_root, agent_config, read_only=True)

        app.push_screen(
            ReviewModal(
                task=task,
                agent_config=agent_config,
                review_agent=review_agent,
                is_reviewing=True,
                is_running=True,
                read_only=True,
                initial_tab="review-agent-output",
            )
        )

        await wait_for_screen(pilot, ReviewModal, timeout=10.0)
        await pilot.pause()
        await pilot.press("escape")
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        assert pilot.app.screen is kanban


def test_extract_review_decision_from_decision_line() -> None:
    output = """
Reasoning:
- Looked at changes
Decision: Approve
"""
    assert extract_review_decision(output) == "approved"


def test_extract_review_decision_prefers_last_decision() -> None:
    output = """
Decision: Reject
...
Decision: Approve
"""
    assert extract_review_decision(output) == "approved"


def test_extract_review_decision_from_signal_tags() -> None:
    output = "<approve summary='Looks good'/>"
    assert extract_review_decision(output) == "approved"
