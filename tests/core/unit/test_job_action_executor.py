from __future__ import annotations

from types import SimpleNamespace
from typing import Any
from unittest.mock import AsyncMock

from kagan.core.commands.job_action_executor import execute_job_action
from kagan.core.models.enums import TaskStatus, TaskType


def _ctx(**kwargs: object) -> Any:
    return SimpleNamespace(**kwargs)


async def test_execute_start_agent_for_auto_task() -> None:
    task = SimpleNamespace(id="task-1", task_type=TaskType.AUTO, status=TaskStatus.IN_PROGRESS)
    task_service = SimpleNamespace(get_task=AsyncMock(return_value=task))
    automation_service = SimpleNamespace(
        spawn_for_task=AsyncMock(return_value=True),
        is_running=lambda _task_id: False,
    )
    ctx = _ctx(task_service=task_service, automation_service=automation_service)

    result = await execute_job_action(
        ctx,
        action="start_agent",
        params={"task_id": "task-1"},
    )

    assert result["success"] is True
    assert result["task_id"] == "task-1"
    assert result["message"] == "Agent start queued"
    assert result["code"] == "START_QUEUED"
    automation_service.spawn_for_task.assert_awaited_once_with(task)


async def test_execute_start_agent_rejects_non_auto_task() -> None:
    task = SimpleNamespace(id="task-1", task_type=TaskType.PAIR, status=TaskStatus.IN_PROGRESS)
    task_service = SimpleNamespace(get_task=AsyncMock(return_value=task))
    automation_service = SimpleNamespace(
        spawn_for_task=AsyncMock(return_value=False),
        is_running=lambda _task_id: False,
    )
    ctx = _ctx(task_service=task_service, automation_service=automation_service)

    result = await execute_job_action(
        ctx,
        action="start_agent",
        params={"task_id": "task-1"},
    )

    assert result["success"] is False
    assert result["task_id"] == "task-1"
    assert result["message"] == "Only AUTO tasks can start agents"
    assert result["code"] == "TASK_TYPE_MISMATCH"
    assert result["next_tool"] == "task_patch"
    assert result["next_arguments"] == {
        "task_id": "task-1",
        "transition": "set_task_type",
        "set": {"task_type": TaskType.AUTO.value},
    }
    assert result["current_task_type"] == TaskType.PAIR.value
    assert "jobs.submit" in result["hint"]
    automation_service.spawn_for_task.assert_not_awaited()


async def test_execute_start_agent_reports_blocked_runtime_state() -> None:
    task = SimpleNamespace(id="task-1", task_type=TaskType.AUTO, status=TaskStatus.IN_PROGRESS)
    blocked_view = SimpleNamespace(
        is_running=False,
        is_reviewing=False,
        is_blocked=True,
        blocked_reason="Waiting on #deadbeef before starting",
        blocked_by_task_ids=("deadbeef",),
        overlap_hints=("src/core.py",),
        blocked_at=None,
    )
    task_service = SimpleNamespace(get_task=AsyncMock(return_value=task))
    automation_service = SimpleNamespace(
        spawn_for_task=AsyncMock(return_value=True),
        is_running=lambda _task_id: False,
    )
    runtime_service = SimpleNamespace(get=lambda _task_id: blocked_view)
    ctx = _ctx(
        task_service=task_service,
        automation_service=automation_service,
        runtime_service=runtime_service,
    )

    result = await execute_job_action(
        ctx,
        action="start_agent",
        params={"task_id": "task-1"},
    )

    assert result["success"] is True
    assert result["code"] == "START_BLOCKED"
    assert "Waiting on #deadbeef" in result["message"]
    assert result["runtime"]["is_blocked"] is True


async def test_execute_start_agent_reports_started_when_runtime_is_running() -> None:
    task = SimpleNamespace(id="task-1", task_type=TaskType.AUTO, status=TaskStatus.IN_PROGRESS)
    running_view = SimpleNamespace(
        is_running=True,
        is_reviewing=False,
        is_blocked=False,
        blocked_reason=None,
        blocked_by_task_ids=(),
        overlap_hints=(),
        blocked_at=None,
    )
    task_service = SimpleNamespace(get_task=AsyncMock(return_value=task))
    automation_service = SimpleNamespace(
        spawn_for_task=AsyncMock(return_value=True),
        is_running=lambda _task_id: False,
    )
    runtime_service = SimpleNamespace(get=lambda _task_id: running_view)
    ctx = _ctx(
        task_service=task_service,
        automation_service=automation_service,
        runtime_service=runtime_service,
    )

    result = await execute_job_action(
        ctx,
        action="start_agent",
        params={"task_id": "task-1"},
    )

    assert result["success"] is True
    assert result["code"] == "STARTED"
    assert result["runtime"]["is_running"] is True


async def test_execute_stop_agent_returns_success_when_running() -> None:
    task = SimpleNamespace(id="task-1")
    task_service = SimpleNamespace(get_task=AsyncMock(return_value=task))
    automation_service = SimpleNamespace(stop_task=AsyncMock(return_value=True))
    ctx = _ctx(task_service=task_service, automation_service=automation_service)

    result = await execute_job_action(
        ctx,
        action="stop_agent",
        params={"task_id": "task-1"},
    )

    assert result["success"] is True
    assert result["task_id"] == "task-1"
    assert result["message"] == "Agent stop queued"
    assert result["code"] == "STOP_QUEUED"
    automation_service.stop_task.assert_awaited_once_with("task-1")


async def test_execute_job_action_rejects_unsupported_action() -> None:
    ctx = _ctx(task_service=SimpleNamespace(), automation_service=SimpleNamespace())

    result = await execute_job_action(
        ctx,
        action="restart_agent",
        params={"task_id": "task-1"},
    )

    assert result == {
        "success": False,
        "message": "Unsupported job action 'restart_agent'",
        "code": "UNSUPPORTED_ACTION",
        "hint": "Use one of: start_agent, stop_agent",
        "next_tool": "job_start",
        "next_arguments": {"task_id": "task-1", "action": "start_agent"},
        "supported_actions": ["start_agent", "stop_agent"],
    }
