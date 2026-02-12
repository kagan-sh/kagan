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
    BlockedSpawnState,
    RunningTaskState,
)
from kagan.core.time import utc_now

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
    blocked_calls: list[tuple[str, str, tuple[str, ...], tuple[str, ...]]] = []
    cleared_calls: list[str] = []

    def _mark_blocked(
        task_id: str,
        *,
        reason: str,
        blocked_by_task_ids: tuple[str, ...] = (),
        overlap_hints: tuple[str, ...] = (),
    ) -> None:
        blocked_calls.append((task_id, reason, tuple(blocked_by_task_ids), tuple(overlap_hints)))

    runtime_service = SimpleNamespace(
        get=lambda _task_id: None,
        running_tasks=lambda: set(),
        mark_started=lambda _task_id: None,
        mark_ended=lambda _task_id: None,
        set_execution=lambda _task_id, _execution_id, _run_count: None,
        attach_running_agent=lambda _task_id, _agent: None,
        attach_review_agent=lambda _task_id, _agent: None,
        clear_review_agent=lambda _task_id: None,
        mark_blocked=_mark_blocked,
        clear_blocked=lambda task_id: cleared_calls.append(task_id),
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
    engine._test_blocked_calls = blocked_calls  # type: ignore[attr-defined]
    engine._test_cleared_calls = cleared_calls  # type: ignore[attr-defined]
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


async def test_conflicting_pending_task_is_blocked_until_blocker_done() -> None:
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
    engine._running["task-a"] = RunningTaskState()

    assert await engine.spawn_for_task(task_b) is True

    assert spawned == []
    assert list(engine._pending_spawn_queue) == []
    assert task_b.status == TaskStatus.BACKLOG
    assert "task-b" in engine._blocked_pending
    blocked_calls = engine._test_blocked_calls  # type: ignore[attr-defined]
    assert blocked_calls
    assert blocked_calls[-1][0] == "task-b"
    assert blocked_calls[-1][2] == ("task-a",)

    task_a.status = TaskStatus.DONE
    await engine._process_status_event("task-a", TaskStatus.IN_PROGRESS, TaskStatus.DONE)

    assert spawned == ["task-b"]
    assert task_b.status == TaskStatus.IN_PROGRESS
    assert "task-b" not in engine._blocked_pending
    cleared_calls = engine._test_cleared_calls  # type: ignore[attr-defined]
    assert "task-b" in cleared_calls


async def test_conflicting_pending_task_resumes_when_blocker_returns_to_backlog() -> None:
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
    engine._running["task-a"] = RunningTaskState()

    assert await engine.spawn_for_task(task_b) is True

    assert spawned == []
    assert task_b.status == TaskStatus.BACKLOG
    assert "task-b" in engine._blocked_pending

    task_a.status = TaskStatus.BACKLOG
    await engine._process_status_event("task-a", TaskStatus.REVIEW, TaskStatus.BACKLOG)

    assert spawned == ["task-b"]
    assert task_b.status == TaskStatus.IN_PROGRESS
    assert "task-b" not in engine._blocked_pending


async def test_stale_in_progress_event_does_not_clear_blocked_backlog_task() -> None:
    task_a = _Task(
        id="task-a",
        status=TaskStatus.IN_PROGRESS,
        title="calculator core",
        description="Touches src/calculator.py",
    )
    task_b = _Task(
        id="task-b",
        status=TaskStatus.BACKLOG,
        title="calculator tests",
        description="Update src/calculator.py and tests/test_calculator.py",
    )
    engine, _spawned = _build_engine(
        tasks_by_id={"task-a": task_a, "task-b": task_b},
        max_concurrent=2,
    )
    engine._blocked_pending["task-b"] = BlockedSpawnState(
        task_id="task-b",
        blocker_task_ids=("task-a",),
        overlap_hints=("src/calculator.py",),
        reason="Waiting on #task-a",
        blocked_at=utc_now(),
    )
    engine._mark_runtime_blocked(
        "task-b",
        reason="Waiting on #task-a",
        blocked_by_task_ids=("task-a",),
        overlap_hints=("src/calculator.py",),
    )

    await engine._process_status_event("task-b", TaskStatus.BACKLOG, TaskStatus.IN_PROGRESS)

    assert "task-b" in engine._blocked_pending
    cleared_calls = engine._test_cleared_calls  # type: ignore[attr-defined]
    assert "task-b" not in cleared_calls
