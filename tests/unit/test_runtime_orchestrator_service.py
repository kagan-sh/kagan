from __future__ import annotations

from dataclasses import dataclass
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, cast
from unittest.mock import AsyncMock

import pytest

from kagan.adapters.db.repositories import TaskRepository
from kagan.core.models.enums import ExecutionStatus, TaskType
from kagan.services.runtime import (
    RuntimeServiceImpl,
    RuntimeTaskPhase,
    RuntimeTaskView,
)

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.adapters.db.repositories import ExecutionRepository
    from kagan.services.projects import ProjectService
    from kagan.services.types import TaskLike


@dataclass(slots=True)
class _Task:
    id: str
    task_type: TaskType


async def _make_service(
    tmp_path: Path,
    *,
    latest_execution: object | None = None,
    log_entries: list[SimpleNamespace] | None = None,
    spawned: bool = False,
    runtime_view: RuntimeTaskView | None = None,
) -> tuple[RuntimeServiceImpl, SimpleNamespace, SimpleNamespace, TaskRepository]:
    automation = SimpleNamespace(
        wait_for_running_agent=AsyncMock(return_value=None),
        spawn_for_task=AsyncMock(return_value=spawned),
    )
    executions = cast(
        "ExecutionRepository",
        SimpleNamespace(
            get_latest_execution_for_task=AsyncMock(return_value=latest_execution),
            get_execution_log_entries=AsyncMock(return_value=log_entries or []),
            update_execution=AsyncMock(return_value=None),
        ),
    )

    repo = TaskRepository(tmp_path / "runtime-orchestrator.db")
    await repo.initialize()
    service = RuntimeServiceImpl(
        project_service=cast("ProjectService", SimpleNamespace()),
        session_factory=repo.session_factory,
        execution_service=executions,
        automation_resolver=lambda: cast("Any", automation),
    )

    if runtime_view is not None and runtime_view.is_running:
        service.mark_started(runtime_view.task_id)
        service.set_execution(
            runtime_view.task_id,
            runtime_view.execution_id,
            runtime_view.run_count,
        )
        if runtime_view.running_agent is not None:
            service.attach_running_agent(runtime_view.task_id, runtime_view.running_agent)
        if runtime_view.review_agent is not None:
            service.attach_review_agent(runtime_view.task_id, runtime_view.review_agent)
        if runtime_view.phase is RuntimeTaskPhase.REVIEWING and runtime_view.review_agent is None:
            service.attach_review_agent(runtime_view.task_id, cast("Any", object()))
    return service, automation, cast("Any", executions), repo


async def test_prepare_auto_output_waiting_runtime_has_no_side_effects(tmp_path: Path) -> None:
    task = _Task(id="auto0001", task_type=TaskType.AUTO)
    runtime_view = RuntimeTaskView(task_id=task.id, phase=RuntimeTaskPhase.RUNNING)
    service, automation, executions, repo = await _make_service(
        tmp_path,
        runtime_view=runtime_view,
    )

    readiness = await service.prepare_auto_output(cast("TaskLike", task))

    assert readiness.can_open_output is True
    assert readiness.is_running is True
    assert readiness.recovered_stale_execution is False
    assert readiness.message == "Agent is starting. Opening output while live stream attaches."
    executions.update_execution.assert_not_awaited()
    automation.spawn_for_task.assert_not_awaited()
    await repo.close()


async def test_prepare_auto_output_prefers_runtime_view_when_view_is_running(
    tmp_path: Path,
) -> None:
    task = _Task(id="auto0001", task_type=TaskType.AUTO)
    runtime_agent = object()
    runtime_view = RuntimeTaskView(
        task_id=task.id,
        phase=RuntimeTaskPhase.RUNNING,
        execution_id="exec-view",
        run_count=2,
        running_agent=cast("Any", runtime_agent),
    )
    service, _automation, _executions, repo = await _make_service(
        tmp_path,
        runtime_view=runtime_view,
    )

    readiness = await service.prepare_auto_output(cast("TaskLike", task))

    assert readiness.is_running is True
    assert readiness.execution_id == "exec-view"
    assert readiness.running_agent is runtime_agent
    await repo.close()


async def test_prepare_auto_output_does_not_fallback_to_automation_when_view_is_missing(
    tmp_path: Path,
) -> None:
    task = _Task(id="auto0001", task_type=TaskType.AUTO)
    service, _automation, _executions, repo = await _make_service(
        tmp_path,
    )

    readiness = await service.prepare_auto_output(cast("TaskLike", task))

    assert readiness.is_running is False
    assert readiness.execution_id is None
    assert readiness.running_agent is None
    assert readiness.message == "No agent logs available for this task"
    await repo.close()


@pytest.mark.parametrize(
    ("status", "logs", "is_running", "should_recover"),
    [
        pytest.param(ExecutionStatus.RUNNING, [SimpleNamespace(logs="")], False, True, id="stale"),
        pytest.param(
            ExecutionStatus.RUNNING,
            [SimpleNamespace(logs='{"messages":[{"content":"ready"}]}')],
            False,
            False,
            id="running-with-logs",
        ),
        pytest.param(ExecutionStatus.RUNNING, [SimpleNamespace(logs="")], True, False, id="live"),
        pytest.param(
            ExecutionStatus.COMPLETED,
            [SimpleNamespace(logs="")],
            False,
            False,
            id="completed",
        ),
    ],
)
async def test_recover_stale_auto_output_only_recovers_for_true_stale_condition(
    tmp_path: Path,
    status: ExecutionStatus,
    logs: list[SimpleNamespace],
    is_running: bool,
    should_recover: bool,
) -> None:
    task = _Task(id="auto0001", task_type=TaskType.AUTO)
    latest = SimpleNamespace(id="exec-1", status=status)
    runtime_view = (
        RuntimeTaskView(task_id=task.id, phase=RuntimeTaskPhase.RUNNING) if is_running else None
    )
    service, automation, executions, repo = await _make_service(
        tmp_path,
        latest_execution=latest,
        log_entries=logs,
        runtime_view=runtime_view,
    )

    recovery = await service.recover_stale_auto_output(cast("TaskLike", task))

    if should_recover:
        executions.update_execution.assert_awaited_once()
        automation.spawn_for_task.assert_awaited_once_with(task)
        assert "Recovered stale execution" in recovery.message
    else:
        executions.update_execution.assert_not_awaited()
        automation.spawn_for_task.assert_not_awaited()
        assert recovery.message == "Stale AUTO output recovery is not required."
    await repo.close()
