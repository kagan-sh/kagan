from __future__ import annotations

import asyncio
import os
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Awaitable, Callable

    from textual.pilot import Pilot
    from textual.screen import Screen

    from kagan.core.models.enums import TaskStatus
    from kagan.tui.app import KaganApp

# CI runners are significantly slower; scale timeouts accordingly.
_CI_MULTIPLIER: float = 5.0 if os.environ.get("CI") else 1.0


def _ci_timeout(timeout: float) -> float:
    return timeout * _CI_MULTIPLIER


def _deadline(timeout: float) -> float:
    return asyncio.get_running_loop().time() + timeout


async def wait_until(
    predicate: Callable[[], bool],
    *,
    timeout: float = 5.0,
    check_interval: float = 0.05,
    description: str = "condition",
) -> None:
    """Wait for a synchronous predicate to become true."""
    timeout = _ci_timeout(timeout)
    deadline = _deadline(timeout)
    while asyncio.get_running_loop().time() < deadline:
        if predicate():
            return
        await asyncio.sleep(check_interval)
    raise TimeoutError(f"Timed out after {timeout}s waiting for {description}")


async def wait_until_async(
    predicate: Callable[[], Awaitable[bool]],
    *,
    timeout: float = 5.0,
    check_interval: float = 0.05,
    description: str = "condition",
) -> None:
    """Wait for an async predicate to become true."""
    timeout = _ci_timeout(timeout)
    deadline = _deadline(timeout)
    while asyncio.get_running_loop().time() < deadline:
        if await predicate():
            return
        await asyncio.sleep(check_interval)
    raise TimeoutError(f"Timed out after {timeout}s waiting for {description}")


async def wait_for_screen(
    pilot: Pilot,
    screen_type: type[Screen],
    timeout: float = 10.0,
    check_interval: float = 0.1,
) -> Screen:
    from textual.css.query import NoMatches
    from textual.widgets import Label

    from kagan.tui.ui.screens.kanban import KanbanScreen
    from kagan.tui.ui.screens.planner import PlannerScreen
    from kagan.tui.ui.widgets.column import KanbanColumn
    from kagan.tui.ui.widgets.header import KaganHeader

    timeout = _ci_timeout(timeout)
    deadline = _deadline(timeout)
    while asyncio.get_running_loop().time() < deadline:
        await pilot.pause()
        current_screen = pilot.app.screen
        if isinstance(current_screen, screen_type):
            expected_agent = pilot.app.config.general.default_worker_agent.strip()
            if isinstance(current_screen, (KanbanScreen, PlannerScreen)):
                try:
                    header = current_screen.query_one(KaganHeader)
                    agent_text = str(header.query_one("#header-agent", Label).content)
                except (LookupError, NoMatches):
                    agent_text = ""
                if expected_agent and not agent_text:
                    await pilot.pause(check_interval)
                    continue

            if isinstance(current_screen, KanbanScreen):
                tasks = await pilot.app.ctx.task_service.list_tasks(
                    project_id=pilot.app.ctx.active_project_id
                )
                if tasks:
                    columns = current_screen.query(KanbanColumn)
                    if not any(column.get_cards() for column in columns):
                        await pilot.pause(check_interval)
                        continue

            return current_screen
        await pilot.pause(check_interval)

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
    deadline = _deadline(timeout)
    while asyncio.get_running_loop().time() < deadline:
        await pilot.pause()
        try:
            pilot.app.screen.query_one(selector)
            return
        except NoMatches:
            await pilot.pause(check_interval)

    raise TimeoutError(f"Widget '{selector}' not found within {timeout}s")


async def wait_for_planner_ready(
    pilot: Pilot,
    timeout: float = 10.0,
    check_interval: float = 0.1,
) -> None:
    from kagan.tui.ui.screens.planner import PlannerScreen

    timeout = _ci_timeout(timeout)
    deadline = _deadline(timeout)
    while asyncio.get_running_loop().time() < deadline:
        # Pump the Textual message queue so AgentReady messages are processed.
        await pilot.pause()
        screen = pilot.app.screen
        if isinstance(screen, PlannerScreen) and screen._state.agent_ready:
            return
        await pilot.pause(check_interval)

    raise TimeoutError(f"Planner agent not ready within {timeout}s")


async def wait_for_text(
    pilot: Pilot,
    text: str,
    timeout: float = 5.0,
    check_interval: float = 0.1,
) -> None:
    timeout = _ci_timeout(timeout)
    deadline = _deadline(timeout)
    while asyncio.get_running_loop().time() < deadline:
        await pilot.pause()
        rendered = str(pilot.app.screen)
        if text in rendered:
            return
        await pilot.pause(check_interval)

    raise TimeoutError(f"Text '{text}' not found within {timeout}s")


async def wait_for_modal(
    pilot: Pilot,
    modal_type: type[Screen],
    timeout: float = 5.0,
    check_interval: float = 0.1,
) -> Screen:
    timeout = _ci_timeout(timeout)
    deadline = _deadline(timeout)
    while asyncio.get_running_loop().time() < deadline:
        await pilot.pause()
        for screen in pilot.app.screen_stack:
            if isinstance(screen, modal_type):
                return screen
        await pilot.pause(check_interval)

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
    deadline = _deadline(timeout)
    last_status = None
    while asyncio.get_running_loop().time() < deadline:
        task = await app.ctx.task_service.get_task(task_id)
        if task:
            last_status = task.status
            if task.status == status:
                return
        if pilot is not None:
            await pilot.pause(0.1)
        else:
            await asyncio.sleep(0.1)
    raise TimeoutError(
        f"Task {task_id} did not reach {status} within {timeout}s (last status: {last_status})"
    )


async def type_text(pilot: Pilot, text: str, *, delay: float = 0.0) -> None:
    for char in text:
        await pilot.press(char)
        if delay > 0:
            await pilot.pause(delay)


async def wait_for_widget_state(
    pilot: Pilot,
    selector: str,
    predicate: Callable[[Any], bool],
    *,
    timeout: float = 5.0,
    check_interval: float = 0.05,
    description: str = "widget state",
) -> Any:
    """Wait for a widget matching ``selector`` to satisfy ``predicate``."""
    from textual.css.query import NoMatches

    timeout = _ci_timeout(timeout)
    deadline = _deadline(timeout)
    while asyncio.get_running_loop().time() < deadline:
        await pilot.pause()
        try:
            widget = pilot.app.screen.query_one(selector)
        except NoMatches:
            await pilot.pause(check_interval)
            continue
        if predicate(widget):
            return widget
        await pilot.pause(check_interval)
    raise TimeoutError(f"Timed out after {timeout}s waiting for {description}: {selector}")
