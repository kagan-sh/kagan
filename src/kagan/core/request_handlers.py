"""Request handlers — convert KaganAPI return values to the dict
shapes that the existing CQRS handlers produce.

Each handler takes ``(api, params)`` and returns a dict matching the
original handler's response shape. Imported by ``host.py`` to populate
the ``_REQUEST_DISPATCH_MAP``.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Any

from kagan.core.commands.job_action_executor import SUPPORTED_JOB_ACTIONS
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

if TYPE_CHECKING:
    from kagan.core.api import KaganAPI
    from kagan.core.models.enums import PairTerminalBackend, TaskPriority, TaskStatus, TaskType

logger = logging.getLogger(__name__)


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
    if isinstance(value, list):
        return [str(item).strip() for item in value if str(item).strip()]
    raise ValueError("acceptance_criteria must be a list of strings")


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
    task = await api.get_task(params["task_id"])
    if task is None:
        return {"found": False, "task": None}
    return {"found": True, "task": task_to_dict(task)}


async def handle_task_list(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    from kagan.core.models.enums import TaskStatus

    project_id = params.get("project_id")
    status: TaskStatus | None = None
    status_filter = _non_empty_str(params.get("filter"))
    if status_filter is not None:
        status = TaskStatus(status_filter.upper())

    tasks = await api.list_tasks(project_id=project_id, status=status)

    excluded_task_ids = set(_str_list(params.get("exclude_task_ids")))
    filtered_tasks = [t for t in tasks if t.id not in excluded_task_ids]

    return {
        "tasks": [task_to_dict(t) for t in filtered_tasks],
        "count": len(filtered_tasks),
    }


async def handle_task_search(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    query = str(params.get("query", "")).strip()
    if not query:
        return {"tasks": [], "count": 0}
    tasks = await f.search_tasks(query)
    return {"tasks": [task_to_dict(t) for t in tasks], "count": len(tasks)}


async def handle_task_scratchpad(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    content = await f.get_scratchpad(task_id)
    return {"task_id": task_id, "content": content}


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
    logs = await f.get_task_logs(task_id, limit=limit)
    return {"task_id": task_id, "logs": logs, "count": len(logs)}


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
    task = await f.update_task(task_id, **fields)
    if task is None:
        return _task_not_found_response(task_id)
    return {"success": True, "task_id": task.id, "code": "UPDATED"}


async def handle_task_move(api: KaganAPI, params: dict[str, Any]) -> dict[str, Any]:
    f = _assert_api(api)
    task_id = params["task_id"]
    new_status = _parse_task_status(params["status"])
    task = await f.move_task(task_id, new_status)
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
    task = await f.approve_task(task_id)
    if task is None:
        return _task_not_found_response(task_id)
    return {"success": True, "task_id": task.id, "status": task.status.value, "code": "APPROVED"}


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
            "next_tool": "jobs_list_actions",
            "next_arguments": {},
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
