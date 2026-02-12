"""Execution helpers for job actions submitted via jobs.submit."""

from __future__ import annotations

from enum import StrEnum
from typing import TYPE_CHECKING, Any

from kagan.core.runtime_helpers import runtime_snapshot_for_task

if TYPE_CHECKING:
    from kagan.core.bootstrap import AppContext


class JobAction(StrEnum):
    START_AGENT = "start_agent"
    STOP_AGENT = "stop_agent"


SUPPORTED_JOB_ACTIONS: frozenset[str] = frozenset(action.value for action in JobAction)


def _runtime_snapshot(ctx: AppContext, task_id: str) -> dict[str, Any]:
    snapshot = dict(
        runtime_snapshot_for_task(
            task_id=task_id,
            runtime_service=getattr(ctx, "runtime_service", None),
        )
    )
    automation = getattr(ctx, "automation_service", None)
    if automation is None:
        return snapshot
    if not bool(snapshot.get("is_running", False)):
        is_running = getattr(automation, "is_running", None)
        if callable(is_running):
            snapshot["is_running"] = bool(is_running(task_id))
    if not bool(snapshot.get("is_reviewing", False)):
        is_reviewing = getattr(automation, "is_reviewing", None)
        if callable(is_reviewing):
            snapshot["is_reviewing"] = bool(is_reviewing(task_id))
    return snapshot


def _task_id_from_params(params: dict[str, Any]) -> str | None:
    task_id = params.get("task_id")
    if not isinstance(task_id, str):
        return None
    stripped = task_id.strip()
    return stripped if stripped else None


def _coerce_job_action(action: str) -> JobAction | None:
    try:
        return JobAction(action)
    except ValueError:
        return None


async def _execute_start_agent(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    from kagan.core.models.enums import TaskType

    task_id = _task_id_from_params(params)
    if task_id is None:
        return {
            "success": False,
            "message": "task_id is required",
            "code": "INVALID_TASK_ID",
        }

    task = await ctx.task_service.get_task(task_id)
    if task is None:
        return {
            "success": False,
            "task_id": task_id,
            "message": f"Task {task_id} not found",
            "code": "TASK_NOT_FOUND",
        }
    if task.task_type is not TaskType.AUTO:
        return {
            "success": False,
            "task_id": task_id,
            "message": "Only AUTO tasks can start agents",
            "code": "TASK_TYPE_MISMATCH",
            "hint": (
                "Set task_type to AUTO before calling jobs.submit with "
                f"action='{JobAction.START_AGENT.value}'."
            ),
            "next_tool": "tasks_update",
            "next_arguments": {"task_id": task_id, "task_type": TaskType.AUTO.value},
            "current_task_type": task.task_type.value,
        }

    started = await ctx.automation_service.spawn_for_task(task)
    runtime = _runtime_snapshot(ctx, task_id)
    runtime_is_running = bool(runtime.get("is_running"))
    runtime_is_blocked = bool(runtime.get("is_blocked"))
    runtime_is_pending = bool(runtime.get("is_pending"))

    if runtime_is_running or ctx.automation_service.is_running(task_id):
        return {
            "success": True,
            "task_id": task_id,
            "message": "Agent running",
            "code": "STARTED",
            "runtime": runtime,
        }

    if runtime_is_blocked:
        return {
            "success": True,
            "task_id": task_id,
            "message": runtime.get("blocked_reason")
            or "Agent start is conflict-blocked and queued for auto-resume",
            "code": "START_BLOCKED",
            "hint": "Task will auto-resume when blocking tasks are no longer active.",
            "runtime": runtime,
        }

    if runtime_is_pending:
        return {
            "success": True,
            "task_id": task_id,
            "message": runtime.get("pending_reason")
            or "Agent start queued for scheduler admission",
            "code": "START_PENDING",
            "runtime": runtime,
        }

    if started:
        return {
            "success": True,
            "task_id": task_id,
            "message": "Agent start queued",
            "code": "START_QUEUED",
            "runtime": runtime,
        }

    return {
        "success": False,
        "task_id": task_id,
        "message": "Agent was not started",
        "code": "NOT_STARTED",
        "runtime": runtime,
    }


async def _execute_stop_agent(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    task_id = _task_id_from_params(params)
    if task_id is None:
        return {
            "success": False,
            "message": "task_id is required",
            "code": "INVALID_TASK_ID",
        }

    task = await ctx.task_service.get_task(task_id)
    if task is None:
        return {
            "success": False,
            "task_id": task_id,
            "message": f"Task {task_id} not found",
            "code": "TASK_NOT_FOUND",
        }

    stopped = await ctx.automation_service.stop_task(task_id)
    runtime = _runtime_snapshot(ctx, task_id)
    return {
        "success": stopped,
        "task_id": task_id,
        "message": "Agent stop queued" if stopped else "No running agent for this task",
        "code": "STOP_QUEUED" if stopped else "NOT_RUNNING",
        "runtime": runtime,
    }


async def execute_job_action(
    ctx: AppContext,
    *,
    action: str,
    params: dict[str, Any],
) -> dict[str, Any]:
    resolved_action = _coerce_job_action(action)
    if resolved_action is JobAction.START_AGENT:
        return await _execute_start_agent(ctx, params)
    if resolved_action is JobAction.STOP_AGENT:
        return await _execute_stop_agent(ctx, params)

    return {
        "success": False,
        "message": f"Unsupported job action '{action}'",
        "code": "UNSUPPORTED_ACTION",
        "hint": "Call jobs_list_actions to discover valid action names.",
        "next_tool": "jobs_list_actions",
        "next_arguments": {},
        "supported_actions": sorted(SUPPORTED_JOB_ACTIONS),
    }


__all__ = ["SUPPORTED_JOB_ACTIONS", "execute_job_action"]
