"""User-facing status change notification behavior in Kanban."""

from __future__ import annotations

import asyncio

import pytest
from tests.helpers.wait import wait_for_screen

from kagan.core.models.enums import TaskStatus
from kagan.ui.screens.kanban import KanbanScreen


@pytest.mark.asyncio
async def test_external_task_status_change_shows_toast(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, KanbanScreen, timeout=10.0)
        app.clear_notifications()

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        task = next(t for t in tasks if t.status == TaskStatus.BACKLOG)

        await app.ctx.task_service.move(task.id, TaskStatus.IN_PROGRESS)

        await pilot.pause()
        await asyncio.sleep(0.25)
        await pilot.pause()

        messages = [n.message for n in app._notifications]
        assert any(
            f"#{task.short_id}" in message and "BACKLOG -> IN PROGRESS" in message
            for message in messages
        )
