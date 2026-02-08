from __future__ import annotations

import asyncio
import logging

from kagan.utils.background_tasks import BackgroundTasks


async def test_background_tasks_spawn_logs_exceptions(caplog) -> None:
    tasks = BackgroundTasks()

    async def _boom() -> None:
        raise RuntimeError("background-failure")

    with caplog.at_level(logging.ERROR):
        tasks.spawn(_boom(), name="boom-task")
        await asyncio.sleep(0)
        await asyncio.sleep(0)

    assert any("Background task failed" in message for message in caplog.messages)
