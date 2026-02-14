"""Request handlers — convert KaganAPI return values to the dict
shapes that the existing CQRS handlers produce.

Each handler takes ``(api, params)`` and returns a dict matching the
original handler's response shape. Imported by ``host.py`` to populate
the ``_REQUEST_DISPATCH_MAP``.
"""

from __future__ import annotations

import dataclasses
import json
import logging
from collections.abc import Awaitable
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, Any

from kagan.core.commands.job_action_executor import SUPPORTED_JOB_ACTIONS
from kagan.core.models.enums import TaskStatus
from kagan.core.request_handler_support import (
    SESSION_PROMPT_PATH,
    build_handoff_payload,
    build_job_response,
    invalid_job_id_response,
    invalid_task_id_response,
    job_not_found_response,
    parse_events_limit,
    parse_events_offset,
    parse_requested_worktree,
    parse_timeout_seconds,
    project_to_dict,
    resolve_pair_backend,
    session_create_error_response,
    task_to_dict,
)
from kagan.core.runtime_helpers import empty_runtime_snapshot, runtime_snapshot_for_task

if TYPE_CHECKING:
    from kagan.core.adapters.db.schema import Task
    from kagan.core.api import KaganAPI
    from kagan.core.models.enums import PairTerminalBackend, TaskPriority, TaskType
    from kagan.core.services.workspaces import RepoWorkspaceInput

logger = logging.getLogger(__name__)
_DEFAULT_TASK_SCRATCHPAD_CHAR_LIMIT = 16_000
_DEFAULT_TASK_LOG_ENTRY_CHAR_LIMIT = 6_000
_DEFAULT_TASK_LOG_TOTAL_CHAR_LIMIT = 18_000
_TUI_ALLOWED_API_METHODS: frozenset[str] = frozenset(
    {
        "abort_workspace_rebase",
        "add_repo",
        "apply_rejection_feedback",
        "attach_session",
        "cancel_job",
        "cleanup_orphan_workspaces",
        "close_exploratory",
        "count_executions_for_task",
        "create_project",
        "create_session",
        "create_task",
        "decide_startup",
        "delete_task",
        "dispatch_runtime_session",
        "find_project_by_repo_path",
        "get_all_diffs",
        "get_execution",
        "get_execution_log_entries",
        "get_latest_execution_for_task",
        "get_project",
        "get_project_repo_details",
        "get_project_repos",
        "get_queue_status",
        "get_queued_messages",
        "get_running_task_ids",
        "get_runtime_view",
        "get_scratchpad",
        "get_task",
        "get_workspace_commit_log",
        "get_workspace_diff",
        "get_workspace_diff_stats",
        "get_workspace_path",
        "get_workspace_repos",
        "get_repo_diff",
        "has_no_changes",
        "is_automation_running",
        "kill_session",
        "list_pending_planner_drafts",
        "list_projects",
        "list_tasks",
        "list_workspaces",
        "merge_repo",
        "merge_task_direct",
        "move_task",
        "open_project",
        "prepare_auto_output",
        "provision_workspace",
        "queue_message",
        "rebase_workspace",
        "reconcile_running_tasks",
        "recover_stale_auto_output",
        "remove_queued_message",
        "resolve_task_base_branch",
        "run_workspace_janitor",
        "runtime_state",
        "save_planner_draft",
        "search_tasks",
        "session_exists",
        "submit_job",
        "take_queued_message",
        "update_planner_draft_status",
        "update_repo_default_branch",
        "update_task",
        "wait_job",
    }
)


def _bounded_int(
    value: object,
    *,
    default: int,
    minimum: int,
    maximum: int,
) -> int:
    if isinstance(value, int) and not isinstance(value, bool):
        return max(minimum, min(value, maximum))
    return default


def _truncate_for_transport(content: str, *, limit: int) -> tuple[str, bool]:
    if limit <= 0:
        return "", bool(content)
    if len(content) <= limit:
        return content, False
    omitted_chars = len(content) - limit
    return f"{content[:limit]}\n\n[truncated {omitted_chars} chars for transport]", True


def _task_not_found_response(task_id: str) -> dict[str, Any]:
    return {
        "success": False,
        "task_id": task_id,
        "message": f"Task {task_id} not found",
        "code": "TASK_NOT_FOUND",
    }


def _assert_api(api: KaganAPI) -> KaganAPI:
    from kagan.core.api import KaganAPI

    assert isinstance(api, KaganAPI)
    return api


def _non_empty_str(value: object) -> str | None:
    match value:
        case str() as text:
            normalized = text.strip()
            return normalized if normalized else None
        case _:
            return None


def _str_list(value: object) -> list[str]:
    match value:
        case list() as values:
            return [text for item in values if (text := str(item).strip())]
        case _:
            return []


def _str_object_dict(value: object) -> dict[str, object] | None:
    match value:
        case dict() as mapping if mapping:
            return {str(key): val for key, val in mapping.items()}
        case _:
            return None


def _isoformat(value: object) -> str | None:
    if isinstance(value, str):
        return value
    isoformat = getattr(value, "isoformat", None)
    if callable(isoformat):
        serialized = isoformat()
        if isinstance(serialized, str):
            return serialized
    return None


def _enum_value(value: object) -> str | None:
    enum_value = getattr(value, "value", None)
    if isinstance(enum_value, str):
        return enum_value
    if isinstance(value, str):
        return value
    return None


def _runtime_snapshot(f: KaganAPI, task_id: str) -> dict[str, Any]:
    runtime_service = getattr(f.ctx, "runtime_service", None)
    snapshot = runtime_snapshot_for_task(task_id=task_id, runtime_service=runtime_service)
    return dict(snapshot)


def _normalize_runtime_snapshot(value: object) -> dict[str, Any] | None:
    if not isinstance(value, dict):
        return None
    snapshot = dict(empty_runtime_snapshot())
    for key in ("is_running", "is_reviewing", "is_blocked", "is_pending"):
        if key in value:
            snapshot[key] = bool(value[key])
    for key in ("blocked_reason", "blocked_at", "pending_reason", "pending_at"):
        if key in value:
            snapshot[key] = _non_empty_str(value.get(key))
    for key in ("blocked_by_task_ids", "overlap_hints"):
        raw = value.get(key)
        if isinstance(raw, list):
            snapshot[key] = [str(item).strip() for item in raw if str(item).strip()]
    return snapshot


def _workspace_to_dict(workspace: object) -> dict[str, Any]:
    return {
        "id": str(getattr(workspace, "id", "")),
        "project_id": _non_empty_str(getattr(workspace, "project_id", None)),
        "task_id": _non_empty_str(getattr(workspace, "task_id", None)),
        "branch_name": str(getattr(workspace, "branch_name", "")),
        "path": str(getattr(workspace, "path", "")),
        "status": _enum_value(getattr(workspace, "status", None)),
        "created_at": _isoformat(getattr(workspace, "created_at", None)),
        "updated_at": _isoformat(getattr(workspace, "updated_at", None)),
    }


def _execution_to_dict(execution: object) -> dict[str, Any]:
    return {
        "id": str(getattr(execution, "id", "")),
        "session_id": _non_empty_str(getattr(execution, "session_id", None)),
        "run_reason": _enum_value(getattr(execution, "run_reason", None)),
        "executor_action": dict(getattr(execution, "executor_action", {}) or {}),
        "status": _enum_value(getattr(execution, "status", None)),
        "exit_code": getattr(execution, "exit_code", None),
        "dropped": bool(getattr(execution, "dropped", False)),
        "started_at": _isoformat(getattr(execution, "started_at", None)),
        "completed_at": _isoformat(getattr(execution, "completed_at", None)),
        "created_at": _isoformat(getattr(execution, "created_at", None)),
        "updated_at": _isoformat(getattr(execution, "updated_at", None)),
        "error": _non_empty_str(getattr(execution, "error", None)),
        "metadata": dict(getattr(execution, "metadata_", {}) or {}),
    }


def _execution_log_entry_to_dict(entry: object) -> dict[str, Any]:
    return {
        "id": str(getattr(entry, "id", "")),
        "execution_process_id": _non_empty_str(getattr(entry, "execution_process_id", None)),
        "logs": str(getattr(entry, "logs", "")),
        "byte_size": int(getattr(entry, "byte_size", 0) or 0),
        "inserted_at": _isoformat(getattr(entry, "inserted_at", None)),
    }


def _runtime_context_to_dict(state: object) -> dict[str, Any]:
    return {
        "project_id": _non_empty_str(getattr(state, "project_id", None)),
        "repo_id": _non_empty_str(getattr(state, "repo_id", None)),
    }


def _startup_decision_to_dict(decision: object) -> dict[str, Any]:
    project_id = _non_empty_str(getattr(decision, "project_id", None))
    preferred_repo_id = _non_empty_str(getattr(decision, "preferred_repo_id", None))
    preferred_path_value = getattr(decision, "preferred_path", None)
    preferred_path = str(preferred_path_value) if preferred_path_value is not None else None
    suggest_cwd = bool(getattr(decision, "suggest_cwd", False))
    cwd_path = _non_empty_str(getattr(decision, "cwd_path", None))
    cwd_is_git_repo = bool(getattr(decision, "cwd_is_git_repo", False))
    should_open_project_raw = getattr(decision, "should_open_project", None)
    should_open_project = (
        bool(should_open_project_raw)
        if should_open_project_raw is not None
        else project_id is not None
    )
    return {
        "project_id": project_id,
        "preferred_repo_id": preferred_repo_id,
        "preferred_path": preferred_path,
        "suggest_cwd": suggest_cwd,
        "cwd_path": cwd_path,
        "cwd_is_git_repo": cwd_is_git_repo,
        "should_open_project": should_open_project,
    }


def _runtime_view_to_dict(
    *,
    task_id: str,
    view: object | None,
    runtime_service: object | None,
) -> dict[str, Any]:
    snapshot = runtime_snapshot_for_task(
        task_id=task_id,
        runtime_service=runtime_service,
    )
    run_count_raw = getattr(view, "run_count", 0) if view is not None else 0
    run_count = (
        run_count_raw
        if isinstance(run_count_raw, int) and not isinstance(run_count_raw, bool)
        else 0
    )
    return {
        "task_id": task_id,
        "phase": _enum_value(getattr(view, "phase", None)),
        "execution_id": _non_empty_str(getattr(view, "execution_id", None)),
        "run_count": run_count,
        "has_running_agent": getattr(view, "running_agent", None) is not None,
        "has_review_agent": getattr(view, "review_agent", None) is not None,
        "runtime": dict(snapshot),
    }


def _workspace_repo_to_dict(repo: object) -> dict[str, Any]:
    if isinstance(repo, dict):
        payload = {str(key): value for key, value in repo.items()}
        for key in ("repo_path", "worktree_path"):
            if payload.get(key) is not None:
                payload[key] = str(payload[key])
        diff_stats = payload.get("diff_stats")
        if isinstance(diff_stats, dict):
            payload["diff_stats"] = {str(key): value for key, value in diff_stats.items()}
        return payload

    return {
        "repo_id": _non_empty_str(getattr(repo, "repo_id", None)),
        "repo_name": _non_empty_str(getattr(repo, "repo_name", None)),
        "repo_path": _non_empty_str(getattr(repo, "repo_path", None)),
        "worktree_path": _non_empty_str(getattr(repo, "worktree_path", None)),
        "target_branch": _non_empty_str(getattr(repo, "target_branch", None)),
        "has_changes": bool(getattr(repo, "has_changes", False)),
        "diff_stats": _str_object_dict(getattr(repo, "diff_stats", None)),
    }


def _queued_message_to_dict(message: object) -> dict[str, Any]:
    metadata = getattr(message, "metadata", None)
    return {
        "content": str(getattr(message, "content", "")),
        "author": _non_empty_str(getattr(message, "author", None)),
        "metadata": _str_object_dict(metadata),
        "queued_at": _isoformat(getattr(message, "queued_at", None)),
    }


def _queue_status_to_dict(status: object) -> dict[str, Any]:
    return {
        "has_queued": bool(getattr(status, "has_queued", False)),
        "queued_at": _isoformat(getattr(status, "queued_at", None)),
        "content_preview": _non_empty_str(getattr(status, "content_preview", None)),
        "author": _non_empty_str(getattr(status, "author", None)),
    }


def _planner_proposal_to_dict(proposal: object) -> dict[str, Any]:
    tasks_json_raw = getattr(proposal, "tasks_json", [])
    todos_json_raw = getattr(proposal, "todos_json", [])
    tasks_json = list(tasks_json_raw) if isinstance(tasks_json_raw, list) else []
    todos_json = list(todos_json_raw) if isinstance(todos_json_raw, list) else []
    return {
        "id": str(getattr(proposal, "id", "")),
        "project_id": _non_empty_str(getattr(proposal, "project_id", None)),
        "repo_id": _non_empty_str(getattr(proposal, "repo_id", None)),
        "tasks_json": tasks_json,
        "todos_json": todos_json,
        "status": _enum_value(getattr(proposal, "status", None)),
        "created_at": _isoformat(getattr(proposal, "created_at", None)),
        "updated_at": _isoformat(getattr(proposal, "updated_at", None)),
    }


def _parse_workspace_repo_inputs(value: object) -> list[RepoWorkspaceInput] | str:
    from kagan.core.services.workspaces import RepoWorkspaceInput

    if not isinstance(value, list) or not value:
        return "repos must be a non-empty list"

    parsed: list[RepoWorkspaceInput] = []
    for item in value:
        if not isinstance(item, dict):
            return "Each repos item must be an object with repo_id, repo_path, and target_branch"
        repo_id = _non_empty_str(item.get("repo_id"))
        repo_path = _non_empty_str(item.get("repo_path"))
        target_branch = _non_empty_str(item.get("target_branch"))
        if repo_id is None or repo_path is None or target_branch is None:
            return "Each repos item must include non-empty repo_id, repo_path, and target_branch"
        parsed.append(
            RepoWorkspaceInput(
                repo_id=repo_id,
                repo_path=repo_path,
                target_branch=target_branch,
            )
        )
    return parsed


def _parse_task_status(value: object) -> TaskStatus:
    from kagan.core.models.enums import TaskStatus

    if isinstance(value, TaskStatus):
        return value
    if isinstance(value, str):
        normalized = value.strip().upper().replace("-", "_").replace(" ", "_")
        if normalized == "INPROGRESS":
            normalized = "IN_PROGRESS"
        if normalized in {"AUTO", "PAIR"}:
            raise ValueError(
                f"Invalid task status value: {value!r}. "
                "AUTO/PAIR are task_type values. "
                "Use task_type='AUTO' or task_type='PAIR' with tasks.update."
            )
        try:
            return TaskStatus(normalized)
        except ValueError as exc:
            raise ValueError(
                f"Invalid task status value: {value!r}. "
                "Expected one of: BACKLOG, IN_PROGRESS, REVIEW, DONE."
            ) from exc
    raise ValueError(
        f"Invalid task status value: {value!r}. "
        "Expected one of: BACKLOG, IN_PROGRESS, REVIEW, DONE."
    )


def _parse_task_priority(value: object) -> TaskPriority:
    from kagan.core.models.enums import TaskPriority

    if isinstance(value, TaskPriority):
        return value
    if isinstance(value, int):
        return TaskPriority(value)
    if isinstance(value, str):
        cleaned = value.strip().upper()
        if cleaned.isdigit():
            return TaskPriority(int(cleaned))
        aliases = {
            "LOW": TaskPriority.LOW,
            "MED": TaskPriority.MEDIUM,
            "MEDIUM": TaskPriority.MEDIUM,
            "HIGH": TaskPriority.HIGH,
        }
        if cleaned in aliases:
            return aliases[cleaned]
    raise ValueError(f"Invalid task priority value: {value!r}. Expected one of: LOW, MEDIUM, HIGH.")


def _parse_task_type(value: object) -> TaskType:
    from kagan.core.models.enums import TaskType

    if isinstance(value, TaskType):
        return value
    if isinstance(value, str):
        normalized = value.strip().upper()
        try:
            return TaskType(normalized)
        except ValueError as exc:
            raise ValueError(
                f"Invalid task type value: {value!r}. Expected one of: AUTO, PAIR."
            ) from exc
    raise ValueError(f"Invalid task type value: {value!r}. Expected one of: AUTO, PAIR.")


def _parse_terminal_backend(value: object) -> PairTerminalBackend | None:
    from kagan.core.models.enums import PairTerminalBackend

    if value is None or isinstance(value, PairTerminalBackend):
        return value
    if isinstance(value, str):
        normalized = value.strip().lower()
        try:
            return PairTerminalBackend(normalized)
        except ValueError as exc:
            raise ValueError(
                f"Invalid terminal backend value: {value!r}. Expected one of: tmux, vscode, cursor."
            ) from exc
    raise ValueError(
        f"Invalid terminal backend value: {value!r}. Expected one of: tmux, vscode, cursor."
    )


def _parse_acceptance_criteria(value: object) -> list[str]:
    if value is None:
        return []
    if isinstance(value, str):
        normalized = value.strip()
        return [normalized] if normalized else []
    if isinstance(value, list):
        return [str(item).strip() for item in value if str(item).strip()]
    raise ValueError("acceptance_criteria must be a string or list of strings")


def _parse_wait_timeout_seconds(
    value: object,
    *,
    default_timeout: int,
    max_timeout: int,
) -> float | str:
    if value is None:
        return float(default_timeout)

    if isinstance(value, bool):
        return "timeout_seconds must be a positive number"

    timeout_seconds: float
    if isinstance(value, int | float):
        timeout_seconds = float(value)
    elif isinstance(value, str):
        normalized = value.strip()
        if not normalized:
            return "timeout_seconds must be a positive number"
        try:
            timeout_seconds = float(normalized)
        except ValueError:
            return "timeout_seconds must be a positive number"
    else:
        return "timeout_seconds must be a positive number"

    if timeout_seconds <= 0:
        return "timeout_seconds must be > 0"
    if timeout_seconds > max_timeout:
        return f"timeout_seconds exceeds server maximum of {max_timeout}s"
    return timeout_seconds


def _parse_wait_for_status_filter(value: object) -> set[str] | str | None:
    from kagan.core.models.enums import TaskStatus

    if value is None:
        return None

    values: list[object]
    if isinstance(value, list):
        values = value
    elif isinstance(value, str):
        normalized = value.strip()
        if not normalized:
            return None
        if normalized.startswith("[") and normalized.endswith("]"):
            try:
                parsed = json.loads(normalized)
            except json.JSONDecodeError:
                return "wait_for_status JSON string must decode to a list of statuses"
            if not isinstance(parsed, list):
                return "wait_for_status JSON string must decode to a list of statuses"
            values = parsed
        else:
            values = [part for part in normalized.split(",") if part.strip()]
    else:
        return "wait_for_status must be a list of status strings"

    valid_statuses = {status.value for status in TaskStatus}
    parsed_statuses: set[str] = set()
    for raw_value in values:
        status = str(raw_value).strip().upper().replace("-", "_").replace(" ", "_")
        if status == "INPROGRESS":
            status = "IN_PROGRESS"
        if status not in valid_statuses:
            return (
                f"Invalid status filter value: {raw_value!r}. "
                f"Expected one of: {', '.join(sorted(valid_statuses))}"
            )
        parsed_statuses.add(status)

    return parsed_statuses


def _parse_queue_lane(value: object) -> str:
    if value is None:
        return "implementation"
    if isinstance(value, str):
        normalized = value.strip().lower()
        if normalized in {"implementation", "review", "planner"}:
            return normalized
    return "lane must be one of: implementation, review, planner"


def _parse_runtime_session_event(value: object):
    from kagan.core.services.runtime import RuntimeSessionEvent

    if isinstance(value, RuntimeSessionEvent):
        return value
    if isinstance(value, str):
        normalized = value.strip().lower().replace("-", "_").replace(" ", "_")
        aliases = {
            "project_selected": RuntimeSessionEvent.PROJECT_SELECTED,
            "repo_selected": RuntimeSessionEvent.REPO_SELECTED,
            "repo_cleared": RuntimeSessionEvent.REPO_CLEARED,
            "reset": RuntimeSessionEvent.RESET,
        }
        if normalized in aliases:
            return aliases[normalized]
    return None


def _parse_proposal_status(value: object):
    from kagan.core.models.enums import ProposalStatus

    if isinstance(value, ProposalStatus):
        return value
    if isinstance(value, str):
        normalized = value.strip().lower()
        try:
            return ProposalStatus(normalized)
        except ValueError:
            return None
    return None


def _parse_json_dict_list(value: object, *, field_name: str) -> list[dict[str, Any]] | str:
    if not isinstance(value, list):
        return f"{field_name} must be a list"
    parsed: list[dict[str, Any]] = []
    for item in value:
        if not isinstance(item, dict):
            return f"{field_name} items must be objects"
        parsed.append({str(key): val for key, val in item.items()})
    return parsed


def _build_update_fields(params: dict[str, Any]) -> dict[str, object]:
    fields: dict[str, object] = {}

    passthrough_keys = {"title", "description", "project_id", "parent_id", "base_branch"}
    for key in passthrough_keys:
        if key in params:
            fields[key] = params[key]

    if "status" in params:
        fields["status"] = _parse_task_status(params["status"])
    if "priority" in params:
        fields["priority"] = _parse_task_priority(params["priority"])
    if "task_type" in params:
        fields["task_type"] = _parse_task_type(params["task_type"])
    if "terminal_backend" in params:
        fields["terminal_backend"] = _parse_terminal_backend(params["terminal_backend"])
    if "agent_backend" in params:
        fields["agent_backend"] = params["agent_backend"]
    if "acceptance_criteria" in params:
        fields["acceptance_criteria"] = _parse_acceptance_criteria(params["acceptance_criteria"])

    return fields


async def handle_task_get(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task = await api.get_task(params["task_id"])
    if task is None:
        return {"found": False, "task": None}
    return {
        "found": True,
        "task": task_to_dict(task, runtime_service=getattr(f.ctx, "runtime_service", None)),
    }


async def handle_task_list(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    from kagan.core.models.enums import TaskStatus

    f = _assert_api(api)
    project_id = params.get("project_id")
    include_scratchpad = bool(params.get("include_scratchpad", False))
    status: TaskStatus | None = None
    status_filter = _non_empty_str(params.get("filter"))
    if status_filter is not None:
        status = TaskStatus(status_filter.upper())

    tasks = await api.list_tasks(project_id=project_id, status=status)

    excluded_task_ids = set(_str_list(params.get("exclude_task_ids")))
    filtered_tasks = [t for t in tasks if t.id not in excluded_task_ids]

    serialized_tasks: list[dict[str, Any]] = []
    for task in filtered_tasks:
        task_payload = task_to_dict(task, runtime_service=getattr(f.ctx, "runtime_service", None))
        if include_scratchpad:
            task_payload["scratchpad"] = await api.get_scratchpad(task.id)
        serialized_tasks.append(task_payload)

    return {
        "tasks": serialized_tasks,
        "count": len(filtered_tasks),
    }


async def handle_task_search(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    query = str(params.get("query", "")).strip()
    if not query:
        return {"tasks": [], "count": 0}
    tasks = await f.search_tasks(query)
    runtime_service = getattr(f.ctx, "runtime_service", None)
    return {
        "tasks": [task_to_dict(t, runtime_service=runtime_service) for t in tasks],
        "count": len(tasks),
    }


async def handle_task_scratchpad(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    content_limit = _bounded_int(
        params.get("content_char_limit"),
        default=_DEFAULT_TASK_SCRATCHPAD_CHAR_LIMIT,
        minimum=256,
        maximum=200_000,
    )
    scratchpad = await f.get_scratchpad(task_id)
    content, truncated = _truncate_for_transport(scratchpad, limit=content_limit)
    return {"task_id": task_id, "content": content, "truncated": truncated}


async def handle_task_context(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    return await f.get_task_context(params["task_id"])


async def handle_task_logs(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    raw_limit = params.get("limit", 5)
    limit = 5
    if isinstance(raw_limit, int) and not isinstance(raw_limit, bool):
        limit = max(1, min(raw_limit, 20))
    offset_value = parse_events_offset(params.get("offset"))
    match offset_value:
        case str() as error_message:
            return {
                "success": False,
                "task_id": task_id,
                "message": error_message,
                "code": "INVALID_OFFSET",
            }
        case _:
            pass
    content_limit = _bounded_int(
        params.get("content_char_limit"),
        default=_DEFAULT_TASK_LOG_ENTRY_CHAR_LIMIT,
        minimum=256,
        maximum=200_000,
    )
    total_limit = _bounded_int(
        params.get("total_char_limit"),
        default=_DEFAULT_TASK_LOG_TOTAL_CHAR_LIMIT,
        minimum=content_limit,
        maximum=1_000_000,
    )
    raw_logs = await f.get_task_logs(task_id, limit=limit, offset=offset_value)
    logs = raw_logs.get("logs", [])

    bounded_logs: list[dict[str, Any]] = []
    truncated = False
    used_chars = 0
    per_entry_overhead = 128
    for log in logs:
        if not isinstance(log, dict):
            continue

        raw_content = str(log.get("content", ""))
        content, entry_truncated = _truncate_for_transport(raw_content, limit=content_limit)
        if entry_truncated:
            truncated = True

        remaining = total_limit - used_chars - per_entry_overhead
        if remaining <= 0:
            truncated = True
            break
        if len(content) > remaining:
            content, _ = _truncate_for_transport(content, limit=remaining)
            truncated = True
        if not content:
            continue

        bounded_log = dict(log)
        bounded_log["content"] = content
        bounded_logs.append(bounded_log)
        used_chars += len(content) + per_entry_overhead

    total_runs_raw = raw_logs.get("total_runs")
    total_runs = total_runs_raw if isinstance(total_runs_raw, int) else offset_value + len(logs)
    page_limit_raw = raw_logs.get("limit")
    page_limit = page_limit_raw if isinstance(page_limit_raw, int) else limit
    returned_runs_raw = raw_logs.get("returned_runs")
    source_returned_runs = returned_runs_raw if isinstance(returned_runs_raw, int) else len(logs)
    next_offset_raw = raw_logs.get("next_offset")
    next_offset = next_offset_raw if isinstance(next_offset_raw, int) else None
    has_more_raw = raw_logs.get("has_more")
    has_more = has_more_raw if isinstance(has_more_raw, bool) else next_offset is not None
    if truncated and len(bounded_logs) < source_returned_runs:
        has_more = True
        next_offset = offset_value + len(bounded_logs)

    return {
        "task_id": task_id,
        "logs": bounded_logs,
        "count": len(bounded_logs),
        "total_runs": total_runs,
        "returned_runs": len(bounded_logs),
        "offset": offset_value,
        "limit": page_limit,
        "has_more": has_more,
        "next_offset": next_offset if has_more else None,
        "truncated": truncated,
    }


async def handle_task_create(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    title = params["title"]
    description = params.get("description", "")
    project_id = params.get("project_id")
    created_by = params.get("created_by")

    fields = _build_update_fields(params)
    fields.pop("project_id", None)
    fields.pop("title", None)
    fields.pop("description", None)

    task = await f.create_task(
        title, description, project_id=project_id, created_by=created_by, **fields
    )
    return {"success": True, "task_id": task.id, "title": task.title, "status": task.status.value}


async def handle_task_update(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    fields = _build_update_fields(params)
    requested_done = fields.get("status") is TaskStatus.DONE
    try:
        task = await f.update_task(task_id, **fields)
    except ValueError as exc:
        if not requested_done:
            raise
        return {
            "success": False,
            "task_id": task_id,
            "message": str(exc),
            "code": "INVALID_STATUS_TRANSITION",
            "hint": "Use review merge (or close no-change flow) from REVIEW to reach DONE.",
        }
    if task is None:
        return _task_not_found_response(task_id)
    return {"success": True, "task_id": task.id, "code": "UPDATED"}


async def handle_task_move(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    new_status = _parse_task_status(params["status"])
    try:
        task = await f.move_task(task_id, new_status)
    except ValueError as exc:
        if new_status is not TaskStatus.DONE:
            raise
        return {
            "success": False,
            "task_id": task_id,
            "message": str(exc),
            "code": "INVALID_STATUS_TRANSITION",
            "hint": "Use review merge (or close no-change flow) from REVIEW to reach DONE.",
        }
    if task is None:
        return _task_not_found_response(task_id)
    return {"success": True, "task_id": task.id, "new_status": task.status.value, "code": "MOVED"}


async def handle_task_delete(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    success, message = await f.delete_task(task_id)
    return {"success": success, "task_id": task_id, "message": message}


async def handle_task_update_scratchpad(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    content = params["content"]
    await f.update_scratchpad(task_id, content)
    return {"success": True, "task_id": task_id}


async def handle_task_wait(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    """Wait for a task status change using event-driven wakeup."""
    import asyncio

    from kagan.core.events import TaskDeleted, TaskStatusChanged, TaskUpdated

    f = _assert_api(api)
    task_id = params["task_id"]

    # Parse timeout
    config = f.ctx.config
    default_timeout = config.general.tasks_wait_default_timeout_seconds
    max_timeout = config.general.tasks_wait_max_timeout_seconds

    parsed_timeout = _parse_wait_timeout_seconds(
        params.get("timeout_seconds"),
        default_timeout=default_timeout,
        max_timeout=max_timeout,
    )
    if isinstance(parsed_timeout, str):
        return {
            "changed": False,
            "timed_out": False,
            "task_id": task_id,
            "code": "INVALID_TIMEOUT",
            "message": parsed_timeout,
        }
    timeout_seconds = parsed_timeout

    # Parse wait_for_status filter
    parsed_wait_for_status = _parse_wait_for_status_filter(params.get("wait_for_status"))
    if isinstance(parsed_wait_for_status, str):
        return {
            "changed": False,
            "timed_out": False,
            "task_id": task_id,
            "code": "INVALID_PARAMS",
            "message": parsed_wait_for_status,
        }
    wait_for_status = parsed_wait_for_status

    # Race-safe guard: from_updated_at
    from_updated_at = _non_empty_str(params.get("from_updated_at"))

    # Get current task state
    task = await f.get_task(task_id)
    if task is None:
        return _task_not_found_response(task_id)

    previous_status = task.status.value
    previous_updated_at = task.updated_at.isoformat()
    runtime_service = getattr(f.ctx, "runtime_service", None)

    # Immediate return: task already in target status
    if wait_for_status is not None and previous_status in wait_for_status:
        return {
            "changed": True,
            "timed_out": False,
            "task_id": task_id,
            "previous_status": previous_status,
            "current_status": previous_status,
            "changed_at": previous_updated_at,
            "task": _compact_task_snapshot(task, runtime_service=runtime_service),
            "code": "ALREADY_AT_STATUS",
            "message": f"Task already at target status {previous_status}",
        }

    # Race-safe: detect changes since from_updated_at
    if from_updated_at is not None and previous_updated_at != from_updated_at:
        if wait_for_status is None or previous_status in wait_for_status:
            return {
                "changed": True,
                "timed_out": False,
                "task_id": task_id,
                "previous_status": None,
                "current_status": previous_status,
                "changed_at": previous_updated_at,
                "task": _compact_task_snapshot(task, runtime_service=runtime_service),
                "code": "CHANGED_SINCE_CURSOR",
                "message": "Task changed since from_updated_at cursor",
            }

    # Event-driven wait using asyncio.Event
    wake_event = asyncio.Event()
    change_info: dict[str, Any] = {}

    def _on_event(event: object) -> None:
        if isinstance(event, TaskStatusChanged) and event.task_id == task_id:
            if wait_for_status is not None and event.to_status.value not in wait_for_status:
                return
            change_info["from_status"] = event.from_status.value
            change_info["to_status"] = event.to_status.value
            change_info["changed_at"] = event.updated_at.isoformat()
            wake_event.set()
        elif isinstance(event, TaskUpdated) and event.task_id == task_id:
            if wait_for_status is not None:
                return
            change_info["changed_at"] = event.updated_at.isoformat()
            wake_event.set()
        elif isinstance(event, TaskDeleted) and event.task_id == task_id:
            change_info["deleted"] = True
            wake_event.set()

    event_bus = f.ctx.event_bus
    event_bus.add_handler(_on_event)
    try:
        try:
            await asyncio.wait_for(wake_event.wait(), timeout=timeout_seconds)
        except TimeoutError:
            return {
                "changed": False,
                "timed_out": True,
                "task_id": task_id,
                "previous_status": previous_status,
                "current_status": previous_status,
                "changed_at": None,
                "task": None,
                "code": "WAIT_TIMEOUT",
                "message": f"No change detected within {timeout_seconds}s",
            }
    except asyncio.CancelledError:
        return {
            "changed": False,
            "timed_out": False,
            "task_id": task_id,
            "previous_status": previous_status,
            "current_status": previous_status,
            "changed_at": None,
            "task": None,
            "code": "WAIT_INTERRUPTED",
            "message": "Wait was interrupted",
        }
    finally:
        event_bus.remove_handler(_on_event)

    # Deleted case
    if change_info.get("deleted"):
        return {
            "changed": True,
            "timed_out": False,
            "task_id": task_id,
            "previous_status": previous_status,
            "current_status": None,
            "changed_at": change_info.get("changed_at"),
            "task": None,
            "code": "TASK_DELETED",
            "message": f"Task {task_id} was deleted during wait",
        }

    # Re-fetch for fresh snapshot
    updated_task = await f.get_task(task_id)
    current_status = (
        updated_task.status.value if updated_task is not None else change_info.get("to_status")
    )
    return {
        "changed": True,
        "timed_out": False,
        "task_id": task_id,
        "previous_status": change_info.get("from_status", previous_status),
        "current_status": current_status,
        "changed_at": change_info.get("changed_at"),
        "task": (
            _compact_task_snapshot(updated_task, runtime_service=runtime_service)
            if updated_task
            else None
        ),
        "code": "TASK_CHANGED",
        "message": f"Task status changed: {previous_status} -> {current_status}",
    }


def _compact_task_snapshot(
    task: Task,
    *,
    runtime_service: object | None = None,
) -> dict[str, Any]:
    """Build a compact task snapshot without large logs/scratchpads."""
    return {
        "id": task.id,
        "title": task.title,
        "status": task.status.value,
        "priority": task.priority.value if task.priority else None,
        "task_type": task.task_type.value if task.task_type else None,
        "project_id": task.project_id,
        "updated_at": task.updated_at.isoformat(),
        "created_at": task.created_at.isoformat(),
        "runtime": dict(
            runtime_snapshot_for_task(
                task_id=task.id,
                runtime_service=runtime_service,
            )
        ),
    }


async def handle_review_request(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    summary = params.get("summary", "")
    task = await f.request_review(task_id, summary)
    if task is None:
        return _task_not_found_response(task_id)
    return {
        "success": True,
        "task_id": task.id,
        "status": task.status.value,
        "code": "REVIEW_REQUESTED",
    }


async def handle_review_approve(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    current_task = await f.get_task(task_id)
    if current_task is None:
        return _task_not_found_response(task_id)
    if current_task.status is not TaskStatus.REVIEW:
        return {
            "success": False,
            "task_id": task_id,
            "message": "Task is not in REVIEW",
            "code": "REVIEW_NOT_READY",
            "hint": "Move task to REVIEW before approving.",
        }

    task = await f.approve_task(task_id)
    if task is None:
        return _task_not_found_response(task_id)
    return {
        "success": True,
        "task_id": task.id,
        "status": "approved",
        "task_status": task.status.value,
        "code": "APPROVED",
    }


async def handle_review_reject(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    feedback = params.get("feedback", "")
    action = params.get("action", "reopen")
    task = await f.reject_task(task_id, feedback, action)
    if task is None:
        return _task_not_found_response(task_id)
    return {"success": True, "task_id": task.id, "status": task.status.value, "code": "REJECTED"}


async def handle_review_merge(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    success, message = await f.merge_task(task_id)
    return {
        "success": success,
        "task_id": task_id,
        "message": message,
        "code": "MERGED" if success else "MERGE_FAILED",
    }


async def handle_review_rebase(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    base_branch = _non_empty_str(params.get("base_branch"))
    success, message, conflict_files = await f.rebase_task(task_id, base_branch=base_branch)
    code = "REBASED" if success else ("REBASE_CONFLICT" if conflict_files else "REBASE_FAILED")
    return {
        "success": success,
        "task_id": task_id,
        "message": message,
        "conflict_files": conflict_files,
        "code": code,
    }


async def handle_job_submit(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id_raw = _non_empty_str(params.get("task_id"))
    action_raw = _non_empty_str(params.get("action"))

    if task_id_raw is None:
        return {
            "success": False,
            "message": "task_id is required",
            "code": "INVALID_TASK_ID",
        }
    if action_raw is None or action_raw not in SUPPORTED_JOB_ACTIONS:
        supported = sorted(SUPPORTED_JOB_ACTIONS)
        unsupported_message = (
            f"Unsupported action {action_raw!r}" if action_raw else "Unsupported action"
        )
        return {
            "success": False,
            "task_id": task_id_raw,
            "message": unsupported_message,
            "code": "UNSUPPORTED_ACTION",
            "hint": f"Use one of: {', '.join(supported)}",
            "next_tool": "job_start",
            "next_arguments": {"task_id": task_id_raw, "action": supported[0]},
            "supported_actions": supported,
        }

    task = await f.get_task(task_id_raw)
    if task is None:
        return _task_not_found_response(task_id_raw)

    arguments = _str_object_dict(params.get("arguments"))
    job = await f.submit_job(
        task_id_raw,
        action_raw,
        arguments=arguments,
    )
    return {
        "success": True,
        "job_id": job.job_id,
        "task_id": job.task_id,
        "action": job.action,
        "status": job.status.value,
        "created_at": job.created_at.isoformat(),
        "updated_at": job.updated_at.isoformat(),
        "code": "JOB_SUBMITTED",
    }


async def handle_job_cancel(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    job_id_raw = _non_empty_str(params.get("job_id"))
    task_id_raw = _non_empty_str(params.get("task_id"))

    if job_id_raw is None:
        return invalid_job_id_response()
    if task_id_raw is None:
        return invalid_task_id_response(job_id_raw)

    cancelled = await f.cancel_job(job_id_raw, task_id=task_id_raw)
    if cancelled is None:
        return job_not_found_response(job_id_raw, task_id_raw)

    return {
        "success": True,
        "job_id": cancelled.job_id,
        "task_id": cancelled.task_id,
        "action": cancelled.action,
        "status": cancelled.status.value,
        "created_at": cancelled.created_at.isoformat(),
        "updated_at": cancelled.updated_at.isoformat(),
        "message": cancelled.message,
        "code": cancelled.code or "JOB_CANCELLED",
    }


async def handle_job_get(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    job_id_raw = _non_empty_str(params.get("job_id"))
    task_id_raw = _non_empty_str(params.get("task_id"))

    if job_id_raw is None:
        return invalid_job_id_response()
    if task_id_raw is None:
        return invalid_task_id_response(job_id_raw)

    job = await f.get_job(job_id_raw, task_id=task_id_raw)
    if job is None:
        return job_not_found_response(job_id_raw, task_id_raw)
    return build_job_response(job)


async def handle_job_wait(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    job_id_raw = _non_empty_str(params.get("job_id"))
    task_id_raw = _non_empty_str(params.get("task_id"))

    if job_id_raw is None:
        return invalid_job_id_response()
    if task_id_raw is None:
        return invalid_task_id_response(job_id_raw)

    timeout_value = parse_timeout_seconds(params.get("timeout_seconds"))
    match timeout_value:
        case str() as error_message:
            return {
                "success": False,
                "job_id": job_id_raw,
                "task_id": task_id_raw,
                "message": error_message,
                "code": "INVALID_TIMEOUT",
            }
        case _:
            pass

    job = await f.wait_job(job_id_raw, task_id=task_id_raw, timeout_seconds=timeout_value)
    if job is None:
        return job_not_found_response(job_id_raw, task_id_raw)
    return build_job_response(job, timed_out=True)


async def handle_job_events(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    job_id_raw = _non_empty_str(params.get("job_id"))
    task_id_raw = _non_empty_str(params.get("task_id"))

    if job_id_raw is None:
        return invalid_job_id_response()
    if task_id_raw is None:
        return invalid_task_id_response(job_id_raw)

    limit_value = parse_events_limit(params.get("limit"))
    match limit_value:
        case str() as error_message:
            return {
                "success": False,
                "job_id": job_id_raw,
                "task_id": task_id_raw,
                "message": error_message,
                "code": "INVALID_LIMIT",
            }
        case _:
            pass
    offset_value = parse_events_offset(params.get("offset"))
    match offset_value:
        case str() as error_message:
            return {
                "success": False,
                "job_id": job_id_raw,
                "task_id": task_id_raw,
                "message": error_message,
                "code": "INVALID_OFFSET",
            }
        case _:
            pass

    events = await f.get_job_events(job_id_raw, task_id=task_id_raw)
    if events is None:
        return job_not_found_response(job_id_raw, task_id_raw)

    total_events = len(events)
    page = events[offset_value : offset_value + limit_value]
    next_offset = offset_value + len(page)
    has_more = next_offset < total_events
    return {
        "success": True,
        "job_id": job_id_raw,
        "task_id": task_id_raw,
        "events": [event.to_dict() for event in page],
        "total_events": total_events,
        "returned_events": len(page),
        "offset": offset_value,
        "limit": limit_value,
        "has_more": has_more,
        "next_offset": next_offset if has_more else None,
    }


async def handle_session_create(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    from kagan.core.api import (
        InvalidWorktreePathError,
        SessionCreateFailedError,
        TaskNotFoundError,
        TaskTypeMismatchError,
        WorkspaceNotFoundError,
    )

    f = _assert_api(api)
    task_id = params["task_id"]
    reuse_if_exists = bool(params.get("reuse_if_exists", True))

    worktree_path, worktree_error = parse_requested_worktree(
        task_id=task_id,
        raw_worktree=params.get("worktree_path"),
    )
    if worktree_error is not None:
        return worktree_error

    result = None
    error_response: dict[str, Any] | None = None
    try:
        result = await f.create_session(
            task_id, worktree_path=worktree_path, reuse_if_exists=reuse_if_exists
        )
    except TaskNotFoundError:
        error_response = _task_not_found_response(task_id)
    except (
        TaskTypeMismatchError,
        WorkspaceNotFoundError,
        InvalidWorktreePathError,
        SessionCreateFailedError,
    ) as exc:
        if isinstance(exc, SessionCreateFailedError):
            logger.warning("Failed to create PAIR session for %s: %s", task_id, exc.__cause__)
        error_response = session_create_error_response(task_id, exc)

    if error_response is not None:
        return error_response

    assert result is not None
    backend = resolve_pair_backend(f.ctx, result.task)
    return build_handoff_payload(
        task_id=task_id,
        backend=backend,
        session_name=result.session_name,
        worktree_path=result.worktree_path,
        already_exists=result.already_exists,
    )


async def handle_session_attach(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    attached = await f.attach_session(task_id)
    return {"success": attached, "message": "Attached" if attached else "Session not found"}


async def handle_session_exists(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    task = await f.get_task(task_id)
    backend = resolve_pair_backend(f.ctx, task)
    worktree_path = await f.ctx.workspace_service.get_path(task_id)
    prompt_path = str(worktree_path / SESSION_PROMPT_PATH) if worktree_path else None
    exists = await f.session_exists(task_id)
    return {
        "task_id": task_id,
        "exists": exists,
        "session_name": f"kagan-{task_id}",
        "backend": backend,
        "worktree_path": str(worktree_path) if worktree_path else None,
        "prompt_path": prompt_path,
    }


async def handle_session_kill(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    await f.kill_session(task_id)
    return {"success": True, "task_id": task_id, "message": "Session terminated"}


async def handle_project_create(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    name = str(params.get("name", ""))
    description = str(params.get("description", "")).strip()
    repo_paths = _str_list(params.get("repo_paths"))

    project_id = await f.create_project(name, description=description, repo_paths=repo_paths)

    return {
        "success": True,
        "project_id": project_id,
        "name": name.strip(),
        "description": description,
        "repo_count": len(repo_paths),
    }


async def handle_project_open(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    project_id = params["project_id"]
    project = await f.open_project(project_id)
    return {"success": True, "project_id": project.id, "name": project.name}


async def handle_project_add_repo(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    project_id = str(params.get("project_id", ""))
    repo_path = str(params.get("repo_path", ""))
    is_primary = bool(params.get("is_primary", False))

    repo_id = await f.add_repo(project_id, repo_path, is_primary=is_primary)
    return {
        "success": True,
        "project_id": project_id.strip(),
        "repo_id": repo_id,
        "repo_path": repo_path.strip(),
    }


async def handle_project_get(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    project_id = params["project_id"]
    project = await f.get_project(project_id)
    if project is None:
        return {"found": False, "project": None}
    return {"found": True, "project": project_to_dict(project)}


async def handle_project_list(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    limit = params.get("limit", 10)
    projects = await api.list_projects(limit=limit)
    return {
        "projects": [project_to_dict(p) for p in projects],
        "count": len(projects),
    }


async def handle_project_repos(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    project_id = params["project_id"]
    repos = await f.get_project_repos(project_id)
    return {
        "repos": [
            {
                "id": r.id,
                "name": r.name,
                "display_name": r.display_name,
                "path": str(r.path),
                "default_branch": r.default_branch,
            }
            for r in repos
        ],
        "count": len(repos),
    }


async def handle_project_find_by_repo_path(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    repo_path = params["repo_path"]
    project = await f.find_project_by_repo_path(repo_path)
    if project is None:
        return {"found": False, "project": None}
    return {"found": True, "project": project_to_dict(project)}


async def handle_project_repo_details(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    project_id = params["project_id"]
    repos = await f.get_project_repo_details(project_id)
    return {"repos": repos, "count": len(repos)}


async def handle_settings_get(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    settings = await api.get_settings()
    return {"settings": settings}


async def handle_settings_update(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    from kagan.core.settings import exposed_settings_snapshot

    f = _assert_api(api)
    raw_fields = _str_object_dict(params.get("fields"))
    if raw_fields is None:
        config = f.ctx.config
        return {
            "success": False,
            "message": "fields must be a non-empty object",
            "updated": {},
            "settings": exposed_settings_snapshot(config),
        }
    success, message, updated = await f.update_settings(raw_fields)
    config = f.ctx.config
    return {
        "success": success,
        "message": message,
        "updated": updated,
        "settings": exposed_settings_snapshot(config),
    }


async def handle_audit_list(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    capability = params.get("capability")
    limit = params.get("limit", 50)
    cursor = params.get("cursor")
    events = await f.list_audit_events(capability=capability, limit=limit, cursor=cursor)
    return {
        "events": [
            {
                "id": e.id,
                "occurred_at": e.occurred_at.isoformat() if e.occurred_at else None,
                "actor_type": e.actor_type,
                "actor_id": e.actor_id,
                "session_id": e.session_id,
                "capability": e.capability,
                "command_name": e.command_name,
                "payload_json": e.payload_json,
                "result_json": e.result_json,
                "success": e.success,
            }
            for e in events
        ],
        "count": len(events),
    }


async def handle_diagnostics_instrumentation(
    api: KaganAPI, params: dict[str, Any]
) -> dict[str, Any]:
    f = _assert_api(api)
    return {"instrumentation": await f.get_instrumentation()}


def _normalize_tui_kwargs(value: object) -> dict[str, Any]:
    if value is None:
        return {}
    if not isinstance(value, dict):
        raise ValueError("kwargs must be an object")
    return {str(key): val for key, val in value.items()}


def _required_non_empty(kwargs: dict[str, Any], key: str) -> str:
    value = _non_empty_str(kwargs.get(key))
    if value is None:
        raise ValueError(f"{key} is required")
    return value


def _parse_tui_lane(value: object) -> str:
    lane = _parse_queue_lane(value)
    if lane not in {"implementation", "review", "planner"}:
        raise ValueError(lane)
    return lane


def _parse_jsonable_task_ids(value: object, *, field_name: str) -> set[str]:
    if not isinstance(value, list | tuple | set):
        raise ValueError(f"{field_name} must be a list of task/workspace IDs")
    parsed = {str(item).strip() for item in value if str(item).strip()}
    return parsed


async def _resolve_task_for_tui_method(
    f: KaganAPI,
    kwargs: dict[str, Any],
) -> Any:
    raw_task_id = kwargs.get("task_id")
    task_id = _non_empty_str(raw_task_id)
    if task_id is None:
        raw_task = kwargs.get("task")
        if isinstance(raw_task, dict):
            task_id = _non_empty_str(raw_task.get("id"))
        elif isinstance(raw_task, str):
            task_id = _non_empty_str(raw_task)
    if task_id is None:
        raise ValueError("task_id is required")
    task = await f.get_task(task_id)
    if task is None:
        raise ValueError(f"Task {task_id} not found")
    return task


def _jsonable(value: object, *, api: KaganAPI | None = None) -> Any:
    if value is None or isinstance(value, str | int | float | bool):
        return value

    from enum import Enum

    if isinstance(value, Enum):
        return value.value
    if isinstance(value, datetime):
        return value.isoformat()
    if isinstance(value, Path):
        return str(value)
    if isinstance(value, dict):
        return {str(key): _jsonable(val, api=api) for key, val in value.items()}
    if isinstance(value, list | tuple):
        return [_jsonable(item, api=api) for item in value]
    if isinstance(value, set):
        return sorted(_jsonable(item, api=api) for item in value)

    if api is not None:
        from kagan.core.adapters.db.schema import (
            ExecutionProcess,
            ExecutionProcessLog,
            Project,
            Task,
            Workspace,
        )

        runtime_service = getattr(api.ctx, "runtime_service", None)
        if isinstance(value, Task):
            return task_to_dict(value, runtime_service=runtime_service)
        if isinstance(value, Project):
            return project_to_dict(value)
        if isinstance(value, Workspace):
            return _workspace_to_dict(value)
        if isinstance(value, ExecutionProcess):
            return _execution_to_dict(value)
        if isinstance(value, ExecutionProcessLog):
            return _execution_log_entry_to_dict(value)

    if hasattr(value, "task_id") and hasattr(value, "phase") and hasattr(value, "run_count"):
        task_id = str(getattr(value, "task_id", ""))
        runtime_service = getattr(api.ctx, "runtime_service", None) if api is not None else None
        return _runtime_view_to_dict(
            task_id=task_id,
            view=value,
            runtime_service=runtime_service,
        )

    if (
        hasattr(value, "can_open_output")
        and hasattr(value, "execution_id")
        and hasattr(value, "output_mode")
    ):
        return {
            "can_open_output": bool(getattr(value, "can_open_output", False)),
            "execution_id": _non_empty_str(getattr(value, "execution_id", None)),
            "is_running": bool(getattr(value, "is_running", False)),
            "recovered_stale_execution": bool(getattr(value, "recovered_stale_execution", False)),
            "message": _non_empty_str(getattr(value, "message", None)),
            "output_mode": _enum_value(getattr(value, "output_mode", None)),
            # Live agent handles are intentionally not exposed over IPC.
            "running_agent": None,
        }

    if hasattr(value, "project_id") and hasattr(value, "repo_id"):
        return _runtime_context_to_dict(value)

    if hasattr(value, "suggest_cwd") and hasattr(value, "cwd_path"):
        return _startup_decision_to_dict(value)

    if (
        hasattr(value, "worktrees_pruned")
        and hasattr(value, "branches_deleted")
        and hasattr(value, "repos_processed")
    ):
        worktrees_pruned = int(getattr(value, "worktrees_pruned", 0) or 0)
        branches_deleted_raw = getattr(value, "branches_deleted", [])
        branches_deleted = (
            [str(item) for item in branches_deleted_raw]
            if isinstance(branches_deleted_raw, list)
            else []
        )
        repos_processed_raw = getattr(value, "repos_processed", [])
        repos_processed = (
            [str(item) for item in repos_processed_raw]
            if isinstance(repos_processed_raw, list)
            else []
        )
        return {
            "worktrees_pruned": worktrees_pruned,
            "branches_deleted": branches_deleted,
            "repos_processed": repos_processed,
            "total_cleaned": worktrees_pruned + len(branches_deleted),
        }

    if dataclasses.is_dataclass(value):
        return _jsonable(dataclasses.asdict(value), api=api)

    model_dump = getattr(value, "model_dump", None)
    if callable(model_dump):
        try:
            dumped = model_dump(mode="json")
        except TypeError:
            dumped = model_dump()
        return _jsonable(dumped, api=api)

    to_dict = getattr(value, "to_dict", None)
    if callable(to_dict):
        return _jsonable(to_dict(), api=api)

    if hasattr(value, "__dict__"):
        return _jsonable(vars(value), api=api)

    return str(value)


async def _dispatch_tui_api_call(
    f: KaganAPI,
    method_name: str,
    kwargs: dict[str, Any],
) -> Any:
    if method_name not in _TUI_ALLOWED_API_METHODS:
        raise ValueError(f"Unsupported TUI API method: {method_name}")

    match method_name:
        case "create_task":
            title = _required_non_empty(kwargs, "title")
            description = str(kwargs.get("description", ""))
            project_id = _non_empty_str(kwargs.get("project_id"))
            created_by = _non_empty_str(kwargs.get("created_by"))
            fields = _build_update_fields(kwargs)
            fields.pop("title", None)
            fields.pop("description", None)
            fields.pop("project_id", None)
            return await f.create_task(
                title,
                description,
                project_id=project_id,
                created_by=created_by,
                **fields,
            )
        case "update_task":
            task_id = _required_non_empty(kwargs, "task_id")
            fields = _build_update_fields(kwargs)
            return await f.update_task(task_id, **fields)
        case "move_task":
            task_id = _required_non_empty(kwargs, "task_id")
            status = _parse_task_status(kwargs.get("status"))
            return await f.move_task(task_id, status)
        case "list_tasks":
            project_id = _non_empty_str(kwargs.get("project_id"))
            status: TaskStatus | None = None
            if "status" in kwargs:
                status = _parse_task_status(kwargs["status"])
            elif "filter" in kwargs:
                filter_value = _non_empty_str(kwargs.get("filter"))
                if filter_value is not None:
                    status = _parse_task_status(filter_value)
            return await f.list_tasks(project_id=project_id, status=status)
        case "search_tasks":
            return await f.search_tasks(str(kwargs.get("query", "")))
        case "decide_startup":
            cwd = _required_non_empty(kwargs, "cwd")
            return await f.decide_startup(Path(cwd))
        case "dispatch_runtime_session":
            event = _parse_runtime_session_event(kwargs.get("event"))
            if event is None:
                raise ValueError(
                    "event must be one of: project_selected, repo_selected, repo_cleared, reset"
                )
            return await f.dispatch_runtime_session(
                event,
                project_id=_non_empty_str(kwargs.get("project_id")),
                repo_id=_non_empty_str(kwargs.get("repo_id")),
            )
        case "runtime_state":
            return f.runtime_state
        case "get_runtime_view":
            task_id = _required_non_empty(kwargs, "task_id")
            return f.get_runtime_view(task_id)
        case "get_running_task_ids":
            return f.get_running_task_ids()
        case "reconcile_running_tasks":
            task_ids = _parse_jsonable_task_ids(kwargs.get("task_ids", []), field_name="task_ids")
            await f.reconcile_running_tasks(sorted(task_ids))
            return None
        case "is_automation_running":
            task_id = _required_non_empty(kwargs, "task_id")
            return f.is_automation_running(task_id)
        case "create_session":
            task_id = _required_non_empty(kwargs, "task_id")
            reuse_if_exists = bool(kwargs.get("reuse_if_exists", True))
            worktree_value = _non_empty_str(kwargs.get("worktree_path"))
            worktree_path = (
                Path(worktree_value).expanduser().resolve(strict=False)
                if worktree_value is not None
                else None
            )
            return await f.create_session(
                task_id,
                worktree_path=worktree_path,
                reuse_if_exists=reuse_if_exists,
            )
        case "queue_message":
            session_id = _required_non_empty(kwargs, "session_id")
            content = _required_non_empty(kwargs, "content")
            lane = _parse_tui_lane(kwargs.get("lane"))
            author = _non_empty_str(kwargs.get("author"))
            metadata = _str_object_dict(kwargs.get("metadata"))
            return await f.queue_message(
                session_id,
                content,
                lane=lane,
                author=author,
                metadata=metadata,
            )
        case "get_queue_status":
            session_id = _required_non_empty(kwargs, "session_id")
            lane = _parse_tui_lane(kwargs.get("lane"))
            return await f.get_queue_status(session_id, lane=lane)
        case "get_queued_messages":
            session_id = _required_non_empty(kwargs, "session_id")
            lane = _parse_tui_lane(kwargs.get("lane"))
            return await f.get_queued_messages(session_id, lane=lane)
        case "take_queued_message":
            session_id = _required_non_empty(kwargs, "session_id")
            lane = _parse_tui_lane(kwargs.get("lane"))
            return await f.take_queued_message(session_id, lane=lane)
        case "remove_queued_message":
            session_id = _required_non_empty(kwargs, "session_id")
            lane = _parse_tui_lane(kwargs.get("lane"))
            index_raw = kwargs.get("index")
            if not isinstance(index_raw, int) or isinstance(index_raw, bool):
                raise ValueError("index must be an integer")
            return await f.remove_queued_message(session_id, index_raw, lane=lane)
        case "provision_workspace":
            task_id = _required_non_empty(kwargs, "task_id")
            parsed_repos = _parse_workspace_repo_inputs(kwargs.get("repos"))
            if isinstance(parsed_repos, str):
                raise ValueError(parsed_repos)
            return await f.provision_workspace(task_id=task_id, repos=parsed_repos)
        case "run_workspace_janitor":
            valid_workspace_ids = _parse_jsonable_task_ids(
                kwargs.get("valid_workspace_ids", []),
                field_name="valid_workspace_ids",
            )
            return await f.run_workspace_janitor(
                valid_workspace_ids,
                prune_worktrees=bool(kwargs.get("prune_worktrees", True)),
                gc_branches=bool(kwargs.get("gc_branches", True)),
            )
        case "cleanup_orphan_workspaces":
            valid_task_ids = _parse_jsonable_task_ids(
                kwargs.get("valid_task_ids", []),
                field_name="valid_task_ids",
            )
            return await f.cleanup_orphan_workspaces(valid_task_ids)
        case "save_planner_draft":
            project_id = _required_non_empty(kwargs, "project_id")
            repo_id = _non_empty_str(kwargs.get("repo_id"))
            tasks_json = _parse_json_dict_list(kwargs.get("tasks_json"), field_name="tasks_json")
            if isinstance(tasks_json, str):
                raise ValueError(tasks_json)
            todos_json_raw = kwargs.get("todos_json")
            todos_json: list[dict[str, Any]] | None = None
            if todos_json_raw is not None:
                parsed_todos = _parse_json_dict_list(todos_json_raw, field_name="todos_json")
                if isinstance(parsed_todos, str):
                    raise ValueError(parsed_todos)
                todos_json = parsed_todos
            return await f.save_planner_draft(
                project_id=project_id,
                repo_id=repo_id,
                tasks_json=tasks_json,
                todos_json=todos_json,
            )
        case "list_pending_planner_drafts":
            project_id = _required_non_empty(kwargs, "project_id")
            repo_id = _non_empty_str(kwargs.get("repo_id"))
            return await f.list_pending_planner_drafts(project_id, repo_id=repo_id)
        case "update_planner_draft_status":
            proposal_id = _required_non_empty(kwargs, "proposal_id")
            status = _parse_proposal_status(kwargs.get("status"))
            if status is None:
                raise ValueError("status must be one of: draft, approved, rejected")
            return await f.update_planner_draft_status(proposal_id, status)
        case "has_no_changes":
            task = await _resolve_task_for_tui_method(f, kwargs)
            return await f.has_no_changes(task)
        case "close_exploratory":
            task = await _resolve_task_for_tui_method(f, kwargs)
            return await f.close_exploratory(task)
        case "merge_task_direct":
            task = await _resolve_task_for_tui_method(f, kwargs)
            return await f.merge_task_direct(task)
        case "apply_rejection_feedback":
            task = await _resolve_task_for_tui_method(f, kwargs)
            feedback = _non_empty_str(kwargs.get("feedback"))
            action = _non_empty_str(kwargs.get("action")) or "reopen"
            return await f.apply_rejection_feedback(task, feedback, action)
        case "resolve_task_base_branch":
            task = await _resolve_task_for_tui_method(f, kwargs)
            return await f.resolve_task_base_branch(task)
        case "prepare_auto_output":
            task = await _resolve_task_for_tui_method(f, kwargs)
            return await f.prepare_auto_output(task)
        case "recover_stale_auto_output":
            task = await _resolve_task_for_tui_method(f, kwargs)
            return await f.recover_stale_auto_output(task)
        case "update_repo_default_branch":
            repo_id = _required_non_empty(kwargs, "repo_id")
            branch = _required_non_empty(kwargs, "branch")
            mark_configured = bool(kwargs.get("mark_configured", False))
            return await f.update_repo_default_branch(
                repo_id,
                branch,
                mark_configured=mark_configured,
            )
        case _:
            method = getattr(f, method_name, None)
            if method is None or not callable(method):
                raise ValueError(f"Unsupported TUI API method: {method_name}")
            result = method(**kwargs)
            if isinstance(result, Awaitable):
                return await result
            return result


async def handle_tui_api_call(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    method_name = _non_empty_str(params.get("method"))
    if method_name is None:
        return {
            "success": False,
            "message": "method is required",
            "code": "INVALID_PARAMS",
        }

    try:
        kwargs = _normalize_tui_kwargs(params.get("kwargs"))
        value = await _dispatch_tui_api_call(f, method_name, kwargs)
    except ValueError as exc:
        return {
            "success": False,
            "method": method_name,
            "message": str(exc),
            "code": "INVALID_PARAMS",
        }

    return {
        "success": True,
        "method": method_name,
        "value": _jsonable(value, api=f),
    }
