from __future__ import annotations

from typing import cast

import pytest
from tests.helpers.wait import wait_for_screen

from kagan.tui.ui.modals.review import ReviewModal
from kagan.tui.ui.modals.task_details_modal import TaskDetailsModal
from kagan.tui.ui.screens.kanban import KanbanScreen


@pytest.mark.asyncio
async def test_review_modal_closes_when_task_is_deleted_externally(
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

        app.push_screen(
            ReviewModal(
                task=task,
                agent_config=agent_config,
                is_reviewing=False,
                is_running=False,
                read_only=True,
                initial_tab="review-summary",
            )
        )
        await wait_for_screen(pilot, ReviewModal, timeout=10.0)

        deleted, _ = await app.ctx.api.delete_task(task.id)
        assert deleted is True

        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        assert pilot.app.screen is kanban


@pytest.mark.asyncio
async def test_task_details_modal_closes_when_task_is_deleted_externally(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        tasks = await app.ctx.api.list_tasks(project_id=app.ctx.active_project_id)
        task = tasks[0]

        app.push_screen(TaskDetailsModal(task=task))
        await wait_for_screen(pilot, TaskDetailsModal, timeout=10.0)

        deleted, _ = await app.ctx.api.delete_task(task.id)
        assert deleted is True

        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        assert pilot.app.screen is kanban
