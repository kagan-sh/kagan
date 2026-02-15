from __future__ import annotations

import asyncio
from typing import cast

import pytest
from tests.helpers.wait import wait_for_modal, wait_for_screen, wait_for_widget

from kagan.tui.ui.modals.branch_select import BaseBranchModal
from kagan.tui.ui.screens.kanban import KanbanScreen
from kagan.tui.ui.widgets.card import TaskCard


@pytest.mark.asyncio
async def test_set_task_branch_modal_opens_and_dismisses(
    e2e_app_with_tasks,
    mock_agent_factory,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async with app.run_test(size=(120, 40)) as pilot:
        screen = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        task = tasks[0]

        await wait_for_widget(pilot, f"#card-{task.id}", timeout=10.0)
        card = screen.query_one(f"#card-{task.id}", TaskCard)
        card.focus()
        await pilot.pause()

        await pilot.press("b")
        modal = cast("BaseBranchModal", await wait_for_modal(pilot, BaseBranchModal, timeout=5.0))
        modal.dismiss(None)
        await pilot.pause()

        await wait_for_screen(pilot, KanbanScreen, timeout=5.0)


@pytest.mark.asyncio
async def test_set_task_branch_modal_opens_when_branch_lookup_is_slow(
    e2e_app_with_tasks,
    mock_agent_factory,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    app = e2e_app_with_tasks
    app._agent_factory = mock_agent_factory

    async def _slow_branch_lookup(_path) -> list[str]:
        loop = asyncio.get_running_loop()
        gate = loop.create_future()
        loop.call_later(0.2, gate.set_result, None)
        await gate
        return ["main", "develop"]

    monkeypatch.setattr(
        "kagan.tui.ui.screens.branch_candidates.list_local_branches",
        _slow_branch_lookup,
    )

    async with app.run_test(size=(120, 40)) as pilot:
        screen = cast("KanbanScreen", await wait_for_screen(pilot, KanbanScreen, timeout=10.0))
        tasks = await app.ctx.task_service.list_tasks(project_id=app.ctx.active_project_id)
        task = tasks[0]

        await wait_for_widget(pilot, f"#card-{task.id}", timeout=10.0)
        card = screen.query_one(f"#card-{task.id}", TaskCard)
        card.focus()
        await pilot.pause()

        await pilot.press("b")
        modal = cast("BaseBranchModal", await wait_for_modal(pilot, BaseBranchModal, timeout=5.0))
        modal.dismiss(None)
        await wait_for_screen(pilot, KanbanScreen, timeout=5.0)
