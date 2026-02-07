"""User-facing Kanban behavior when starting AUTO agents."""

from __future__ import annotations

import asyncio
from pathlib import Path
from unittest.mock import AsyncMock

import pytest
from tests.helpers.wait import wait_for_screen

from kagan.core.models.enums import CardIndicator, TaskStatus, TaskType
from kagan.ui.screens.kanban import KanbanScreen
from kagan.ui.widgets.card import TaskCard


@pytest.mark.asyncio
async def test_start_agent_updates_card_indicator_immediately(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = await wait_for_screen(pilot, KanbanScreen, timeout=10.0)

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        target = next(task for task in tasks if task.status == TaskStatus.IN_PROGRESS)
        await app.ctx.task_service.update_fields(target.id, task_type=TaskType.AUTO)

        monkeypatch.setattr(
            app.ctx.workspace_service,
            "get_path",
            AsyncMock(return_value=Path(app.project_root)),
        )
        monkeypatch.setattr(
            app.ctx.automation_service,
            "spawn_for_task",
            AsyncMock(return_value=True),
        )

        await asyncio.sleep(0.25)
        await pilot.pause()

        card = kanban.query_one(f"#card-{target.id}", TaskCard)
        card.focus()
        await pilot.pause()
        assert card.indicator == CardIndicator.IDLE

        await pilot.press("a")

        for _ in range(60):
            await pilot.pause()
            if card.indicator == CardIndicator.RUNNING:
                break
            await asyncio.sleep(0.1)

        assert card.indicator == CardIndicator.RUNNING


@pytest.mark.asyncio
async def test_done_task_shows_passed_indicator(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        kanban = await wait_for_screen(pilot, KanbanScreen, timeout=10.0)

        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        target = next(task for task in tasks if task.status == TaskStatus.IN_PROGRESS)
        await app.ctx.task_service.update_fields(target.id, task_type=TaskType.AUTO)
        await app.ctx.task_service.move(target.id, TaskStatus.DONE)

        await asyncio.sleep(0.25)
        await pilot.pause()

        card = kanban.query_one(f"#card-{target.id}", TaskCard)
        assert card.indicator == CardIndicator.PASSED
