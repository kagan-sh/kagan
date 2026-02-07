from __future__ import annotations

import asyncio
import os
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from textual.pilot import Pilot
    from textual.screen import Screen

    from kagan.app import KaganApp
    from kagan.core.models.enums import TaskStatus

# CI runners are significantly slower; scale timeouts accordingly.
_CI_MULTIPLIER: float = 3.0 if os.environ.get("CI") else 1.0


def _ci_timeout(timeout: float) -> float:
    return timeout * _CI_MULTIPLIER


async def wait_for_screen(
    pilot: Pilot,
    screen_type: type[Screen],
    timeout: float = 10.0,
    check_interval: float = 0.1,
) -> Screen:
    timeout = _ci_timeout(timeout)
    elapsed = 0.0
    while elapsed < timeout:
        await pilot.pause()
        current_screen = pilot.app.screen
        if isinstance(current_screen, screen_type):
            return current_screen
        await asyncio.sleep(check_interval)
        elapsed += check_interval

    current_screen = pilot.app.screen
    assert isinstance(current_screen, screen_type), (
        f"Expected screen {screen_type.__name__}, got {type(current_screen).__name__}"
    )
    return current_screen


async def wait_for_widget(
    pilot: Pilot,
    selector: str,
    timeout: float = 5.0,
    check_interval: float = 0.1,
) -> None:
    from textual.css.query import NoMatches

    timeout = _ci_timeout(timeout)
    await pilot.pause()
    await pilot.pause()

    elapsed = 0.0
    while elapsed < timeout:
        await pilot.pause()
        try:
            pilot.app.screen.query_one(selector)
            return
        except NoMatches:
            await asyncio.sleep(check_interval)
            elapsed += check_interval
            await pilot.pause()

    raise TimeoutError(f"Widget '{selector}' not found within {timeout}s")


async def wait_for_planner_ready(
    pilot: Pilot,
    timeout: float = 10.0,
    check_interval: float = 0.1,
) -> None:
    from kagan.ui.screens.planner import PlannerScreen

    timeout = _ci_timeout(timeout)
    elapsed = 0.0
    while elapsed < timeout:
        await pilot.pause()
        screen = pilot.app.screen
        if isinstance(screen, PlannerScreen) and screen._state.agent_ready:
            return
        await asyncio.sleep(check_interval)
        elapsed += check_interval

    raise TimeoutError(f"Planner agent not ready within {timeout}s")


async def wait_for_workers(
    pilot: Pilot,
    timeout: float = 10.0,
    check_interval: float = 0.1,
) -> None:
    from textual.worker import WorkerState

    timeout = _ci_timeout(timeout)
    elapsed = 0.0
    while elapsed < timeout:
        await pilot.pause()
        workers = list(pilot.app.workers._workers)
        running = [w for w in workers if w.state in (WorkerState.PENDING, WorkerState.RUNNING)]
        if not running:
            for _ in range(3):
                await pilot.pause()
            return
        await asyncio.sleep(check_interval)
        elapsed += check_interval

    await pilot.pause()
    raise TimeoutError(f"Workers did not complete within {timeout}s")


async def wait_for_text(
    pilot: Pilot,
    text: str,
    timeout: float = 5.0,
    check_interval: float = 0.1,
) -> None:
    timeout = _ci_timeout(timeout)
    elapsed = 0.0
    while elapsed < timeout:
        await pilot.pause()
        rendered = str(pilot.app.screen)
        if text in rendered:
            return
        await asyncio.sleep(check_interval)
        elapsed += check_interval

    raise TimeoutError(f"Text '{text}' not found within {timeout}s")


async def wait_for_modal(
    pilot: Pilot,
    modal_type: type[Screen],
    timeout: float = 5.0,
    check_interval: float = 0.1,
) -> Screen:
    timeout = _ci_timeout(timeout)
    await pilot.pause()
    await pilot.pause()

    elapsed = 0.0
    while elapsed < timeout:
        await pilot.pause()
        for screen in pilot.app.screen_stack:
            if isinstance(screen, modal_type):
                await pilot.pause()
                return screen
        await asyncio.sleep(check_interval)
        elapsed += check_interval

    raise TimeoutError(f"Modal {modal_type.__name__} not found within {timeout}s")


async def wait_for_task_status(
    app: KaganApp,
    task_id: str,
    status: TaskStatus,
    *,
    timeout: float = 20.0,
    pilot: Pilot | None = None,
) -> None:
    """Wait for a task to reach a specific status.

    If *pilot* is provided, pumps Textual's message queue each iteration
    so that screen callbacks (e.g. merge after ReviewModal dismiss) execute.
    """
    timeout = _ci_timeout(timeout)
    elapsed = 0.0
    last_status = None
    while elapsed < timeout:
        task = await app.ctx.task_service.get_task(task_id)
        if task:
            last_status = task.status
            if task.status == status:
                return
        if pilot is not None:
            await pilot.pause()
        await asyncio.sleep(0.1)
        elapsed += 0.1
    raise TimeoutError(
        f"Task {task_id} did not reach {status} within {timeout}s (last status: {last_status})"
    )


async def type_text(pilot: Pilot, text: str, *, delay: float = 0.0) -> None:
    for char in text:
        await pilot.press(char)
        if delay > 0:
            await asyncio.sleep(delay)
