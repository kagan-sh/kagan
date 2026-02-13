from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, cast
from unittest.mock import AsyncMock

from tests.helpers.mocks import create_test_config
from tests.helpers.wait import wait_until

from kagan.core.models.enums import TaskStatus, TaskType
from kagan.core.services.automation.runner import (
    AutomationEngine,
    RunningTaskState,
)

if TYPE_CHECKING:
    from kagan.core.agents.agent_factory import AgentFactory
    from kagan.core.services.runtime import RuntimeService
    from kagan.core.services.tasks import TaskService
    from kagan.core.services.types import TaskLike
    from kagan.core.services.workspaces import WorkspaceService


@dataclass(slots=True)
class _Task:
    id: str
    task_type: TaskType = TaskType.AUTO
    status: TaskStatus = TaskStatus.IN_PROGRESS
    agent_backend: str | None = None
    base_branch: str | None = None
    title: str = "AUTO task"
    description: str = ""
    acceptance_criteria: list[str] = field(default_factory=list)

    def get_agent_config(self, config) -> None:
        del config
        return None


def _build_engine(
    *,
    tasks_by_id: dict[str, _Task],
    max_concurrent: int,
) -> tuple[AutomationEngine, list[str]]:
    async def _get_task(task_id: str) -> _Task | None:
        return tasks_by_id.get(task_id)

    async def _update_fields(task_id: str, **kwargs):
        task = tasks_by_id.get(task_id)
        if task is None:
            return None
        for key, value in kwargs.items():
            setattr(task, key, value)
        return task

    scratchpads: dict[str, str] = {task_id: "" for task_id in tasks_by_id}

    async def _get_scratchpad(task_id: str) -> str:
        return scratchpads.get(task_id, "")

    async def _update_scratchpad(task_id: str, content: str) -> None:
        scratchpads[task_id] = content

    task_service = SimpleNamespace(
        get_task=AsyncMock(side_effect=_get_task),
        update_fields=AsyncMock(side_effect=_update_fields),
        get_scratchpad=AsyncMock(side_effect=_get_scratchpad),
        update_scratchpad=AsyncMock(side_effect=_update_scratchpad),
    )

    runtime_service = SimpleNamespace(
        get=lambda _task_id: None,
        running_tasks=lambda: set(),
        mark_started=lambda _task_id: None,
        mark_ended=lambda _task_id: None,
        set_execution=lambda _task_id, _execution_id, _run_count: None,
        attach_running_agent=lambda _task_id, _agent: None,
        attach_review_agent=lambda _task_id, _agent: None,
        clear_review_agent=lambda _task_id: None,
        mark_pending=lambda _task_id, reason: None,
        clear_pending=lambda _task_id: None,
    )
    engine = AutomationEngine(
        task_service=cast("TaskService", task_service),
        workspace_service=cast("WorkspaceService", SimpleNamespace()),
        config=create_test_config(max_concurrent=max_concurrent),
        runtime_service=cast("RuntimeService", runtime_service),
        agent_factory=cast("AgentFactory", lambda *_args, **_kwargs: None),
    )

    spawned: list[str] = []

    async def _fake_spawn(task: TaskLike) -> None:
        spawned.append(task.id)
        task.status = TaskStatus.IN_PROGRESS
        engine._running[task.id] = RunningTaskState()

    cast("Any", engine)._spawn = _fake_spawn
    return engine, spawned


async def test_spawn_is_deferred_at_capacity_and_progresses_after_release() -> None:
    task_b = _Task(id="task-b")
    engine, spawned = _build_engine(
        tasks_by_id={"task-b": task_b},
        max_concurrent=1,
    )
    engine._running["task-a"] = RunningTaskState()

    queued = await engine.spawn_for_task(task_b)
    assert queued is True

    assert spawned == []
    assert list(engine._pending_spawn_queue) == ["task-b"]

    await engine._remove_running_state("task-a")

    assert spawned == ["task-b"]
    assert list(engine._pending_spawn_queue) == []


async def test_pending_spawn_queue_is_fifo_and_deduped() -> None:
    task_b = _Task(id="task-b")
    task_c = _Task(id="task-c")
    engine, spawned = _build_engine(
        tasks_by_id={"task-b": task_b, "task-c": task_c},
        max_concurrent=1,
    )
    engine._running["task-a"] = RunningTaskState()

    assert await engine.spawn_for_task(task_b) is True
    assert await engine.spawn_for_task(task_c) is True
    assert await engine.spawn_for_task(task_b) is True

    assert spawned == []
    assert list(engine._pending_spawn_queue) == ["task-b", "task-c"]

    await engine._remove_running_state("task-a")
    assert spawned == ["task-b"]
    assert list(engine._pending_spawn_queue) == ["task-c"]

    await engine._remove_running_state("task-b")
    assert spawned == ["task-b", "task-c"]
    assert list(engine._pending_spawn_queue) == []


async def test_duplicate_spawn_requests_are_accepted_but_enqueued_once() -> None:
    task_b = _Task(id="task-b")
    engine, spawned = _build_engine(
        tasks_by_id={"task-b": task_b},
        max_concurrent=1,
    )
    engine._running["task-a"] = RunningTaskState()

    assert await engine.spawn_for_task(task_b) is True
    assert await engine.spawn_for_task(task_b) is True

    assert spawned == []
    assert list(engine._pending_spawn_queue) == ["task-b"]


async def test_stop_running_task_persists_backlog_and_emits_status_event() -> None:
    task_a = _Task(id="task-a", status=TaskStatus.IN_PROGRESS)
    engine, _spawned = _build_engine(
        tasks_by_id={"task-a": task_a},
        max_concurrent=1,
    )
    engine._running["task-a"] = RunningTaskState()

    stopped = await engine.stop_task("task-a")

    assert stopped is True
    assert task_a.status is TaskStatus.BACKLOG
    event = await asyncio.wait_for(engine._event_queue.get(), timeout=0.2)
    assert event.task_id == "task-a"
    assert event.old_status is TaskStatus.IN_PROGRESS
    assert event.new_status is TaskStatus.BACKLOG


async def test_callback_path_drains_pending_spawns_when_slot_frees() -> None:
    task_b = _Task(id="task-b")
    engine, spawned = _build_engine(
        tasks_by_id={"task-b": task_b},
        max_concurrent=1,
    )
    engine._running["task-a"] = RunningTaskState()

    assert await engine.spawn_for_task(task_b) is True

    assert spawned == []
    engine._remove_running_state_soon("task-a")

    await wait_until(
        lambda: len(spawned) == 1,
        timeout=0.5,
        check_interval=0.01,
        description="pending spawn queue drains after slot release",
    )
    assert spawned == ["task-b"]


async def test_overlapping_descriptions_spawn_in_parallel() -> None:
    """Verify tasks with overlapping text hints spawn in parallel.

    Previously, tasks mentioning the same files (e.g., src/calculator.py) would
    be blocked from parallel execution. Now, conflicts are handled at merge time
    via rebase, so tasks should spawn unconditionally when capacity is available.
    """
    task_a = _Task(
        id="task-a",
        status=TaskStatus.IN_PROGRESS,
        title="calculator core",
        description="Touches src/calculator.py",
    )
    task_b = _Task(
        id="task-b",
        status=TaskStatus.IN_PROGRESS,
        title="calculator tests",
        description="Update src/calculator.py and tests/test_calculator.py",
    )
    engine, spawned = _build_engine(
        tasks_by_id={"task-a": task_a, "task-b": task_b},
        max_concurrent=2,
    )

    # Both tasks should spawn immediately when capacity allows
    assert await engine.spawn_for_task(task_a) is True
    assert await engine.spawn_for_task(task_b) is True

    # Both tasks should have spawned (no overlap blocking)
    assert "task-a" in spawned
    assert "task-b" in spawned
    assert len(spawned) == 2


async def test_multiple_tasks_spawn_up_to_capacity_limit() -> None:
    """Verify multiple tasks spawn up to capacity regardless of content overlap."""
    task_a = _Task(id="task-a", title="tests", description="Run tests")
    task_b = _Task(id="task-b", title="tests", description="Run tests")
    task_c = _Task(id="task-c", title="tests", description="Run tests")
    engine, spawned = _build_engine(
        tasks_by_id={"task-a": task_a, "task-b": task_b, "task-c": task_c},
        max_concurrent=2,
    )

    # Spawn all three tasks
    assert await engine.spawn_for_task(task_a) is True
    assert await engine.spawn_for_task(task_b) is True
    assert await engine.spawn_for_task(task_c) is True

    # Two should have spawned (capacity limit), one should be pending
    assert len(spawned) == 2
    assert list(engine._pending_spawn_queue) == ["task-c"]

    # Release one slot
    await engine._remove_running_state(spawned[0])

    # Third task should now spawn
    assert "task-c" in spawned
    assert len(spawned) == 3
    assert list(engine._pending_spawn_queue) == []
