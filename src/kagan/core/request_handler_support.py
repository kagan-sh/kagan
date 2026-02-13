from __future__ import annotations

import shlex
from pathlib import Path
from typing import TYPE_CHECKING, Any
from urllib.parse import quote

from kagan.core.services.jobs import JobStatus

if TYPE_CHECKING:
    from kagan.core.adapters.db.schema import Project, Task
    from kagan.core.bootstrap import AppContext
    from kagan.core.services.jobs import JobRecord

TMUX_DOCS_URL = "https://github.com/tmux/tmux/wiki"
SESSION_PROMPT_PATH = Path(".kagan/start_prompt.md")

DEFAULT_EVENTS_LIMIT = 50
MAX_EVENTS_LIMIT = 100


def task_to_dict(task: Task) -> dict[str, Any]:
    """Convert a Task domain object to the canonical dict representation."""
    return {
        "id": task.id,
        "project_id": task.project_id,
        "parent_id": task.parent_id,
        "title": task.title,
        "description": task.description,
        "status": task.status.value,
        "priority": task.priority.value if task.priority else None,
        "task_type": task.task_type.value if task.task_type else None,
        "terminal_backend": task.terminal_backend.value if task.terminal_backend else None,
        "agent_backend": task.agent_backend,
        "acceptance_criteria": task.acceptance_criteria,
        "base_branch": task.base_branch,
        "created_at": task.created_at.isoformat(),
        "updated_at": task.updated_at.isoformat(),
    }


def project_to_dict(project: Project) -> dict[str, Any]:
    """Convert a Project domain object to the canonical dict representation."""
    return {
        "id": project.id,
        "name": project.name,
        "description": project.description,
    }


def resolve_pair_backend(ctx: AppContext, task: object | None) -> str:
    """Resolve the PAIR terminal backend from task -> config -> default (tmux)."""
    from kagan.core.models.enums import resolve_pair_backend

    task_backend = getattr(task, "terminal_backend", None)
    config_backend = getattr(ctx.config.general, "default_pair_terminal_backend", None)
    return resolve_pair_backend(task_backend, config_backend)


def build_handoff_payload(
    *,
    task_id: str,
    backend: str,
    session_name: str,
    worktree_path: Path,
    already_exists: bool,
) -> dict[str, Any]:
    """Build the rich handoff dict with terminal commands/links/instructions."""
    worktree_str = str(worktree_path)
    prompt_path = worktree_path / SESSION_PROMPT_PATH
    prompt_str = str(prompt_path)
    quoted_worktree = shlex.quote(worktree_str)
    quoted_prompt = shlex.quote(prompt_str)

    links: dict[str, str] = {
        "worktree_file_url": worktree_path.as_uri(),
        "prompt_file_url": prompt_path.as_uri(),
    }

    if backend == "tmux":
        primary_command = f"tmux attach-session -t {session_name}"
        commands = [
            primary_command,
            "Detach and return to Kagan: Ctrl+b d",
        ]
        links["tmux_docs"] = TMUX_DOCS_URL
        instructions = (
            "Open a terminal and run the attach command. "
            "When finished, detach with Ctrl+b d and continue in Kagan."
        )
    elif backend == "vscode":
        primary_command = f"code --new-window {quoted_worktree} {quoted_prompt}"
        commands = [
            primary_command,
            f"Open startup prompt: cat {quoted_prompt}",
        ]
        links["vscode_prompt_uri"] = f"vscode://file/{quote(prompt_path.as_posix())}"
        instructions = (
            "Open VS Code with the command above, then paste the startup prompt into chat."
        )
    elif backend == "cursor":
        primary_command = f"cursor --new-window {quoted_worktree} {quoted_prompt}"
        commands = [
            primary_command,
            f"Open startup prompt: cat {quoted_prompt}",
        ]
        links["cursor_prompt_uri"] = f"cursor://file/{quote(prompt_path.as_posix())}"
        instructions = (
            "Open Cursor with the command above, then paste the startup prompt into chat."
        )
    else:
        primary_command = f"Open worktree: {worktree_str}"
        commands = [primary_command]
        instructions = "Open the worktree and continue coding in your preferred terminal/editor."

    return {
        "success": True,
        "task_id": task_id,
        "session_name": session_name,
        "backend": backend,
        "already_exists": already_exists,
        "worktree_path": worktree_str,
        "prompt_path": prompt_str,
        "primary_command": primary_command,
        "commands": commands,
        "links": links,
        "instructions": instructions,
        "next_step": "Reply 'ready' when attached so the agent can continue orchestration.",
    }


def parse_requested_worktree(
    *,
    task_id: str,
    raw_worktree: object,
) -> tuple[Path | None, dict[str, Any] | None]:
    if raw_worktree is not None and not isinstance(raw_worktree, str):
        return None, {
            "success": False,
            "task_id": task_id,
            "message": "worktree_path must be a string path",
            "code": "INVALID_WORKTREE_PATH",
        }
    if isinstance(raw_worktree, str) and raw_worktree.strip():
        return Path(raw_worktree.strip()).expanduser().resolve(strict=False), None
    return None, None


def session_create_error_response(task_id: str, exc: Exception) -> dict[str, Any]:
    from kagan.core.api import (
        InvalidWorktreePathError,
        SessionCreateFailedError,
        TaskTypeMismatchError,
        WorkspaceNotFoundError,
    )

    if isinstance(exc, TaskTypeMismatchError):
        from kagan.core.models.enums import TaskType

        return {
            "success": False,
            "task_id": task_id,
            "message": str(exc),
            "code": "TASK_TYPE_MISMATCH",
            "hint": "Set task_type to PAIR before opening a PAIR session.",
            "next_tool": "task_patch",
            "next_arguments": {
                "task_id": task_id,
                "transition": "set_task_type",
                "set": {"task_type": TaskType.PAIR.value},
            },
            "current_task_type": exc.current_task_type,
        }
    if isinstance(exc, WorkspaceNotFoundError):
        return {
            "success": False,
            "task_id": task_id,
            "message": f"No workspace found for task {task_id}",
            "code": "WORKSPACE_NOT_FOUND",
            "hint": "Create/activate a workspace for this task before creating a PAIR session.",
        }
    if isinstance(exc, InvalidWorktreePathError):
        return {
            "success": False,
            "task_id": task_id,
            "message": str(exc),
            "code": "INVALID_WORKTREE_PATH",
            "hint": "Use session_manage(action='read') to inspect expected worktree_path.",
            "next_tool": "session_manage",
            "next_arguments": {"action": "read", "task_id": task_id},
        }
    if isinstance(exc, SessionCreateFailedError):
        return {
            "success": False,
            "task_id": task_id,
            "message": str(exc),
            "code": "SESSION_CREATE_FAILED",
            "hint": "Confirm workspace path and terminal backend, then retry session_manage(open).",
        }
    msg = f"Unsupported session_create exception: {type(exc).__name__}"
    raise TypeError(msg)


def invalid_job_id_response() -> dict[str, Any]:
    return {
        "success": False,
        "message": "job_id is required",
        "code": "INVALID_JOB_ID",
    }


def invalid_task_id_response(job_id: str) -> dict[str, Any]:
    return {
        "success": False,
        "job_id": job_id,
        "message": "task_id is required",
        "code": "INVALID_TASK_ID",
    }


def job_not_found_response(job_id: str, task_id: str) -> dict[str, Any]:
    return {
        "success": False,
        "job_id": job_id,
        "task_id": task_id,
        "message": "Job not found",
        "code": "JOB_NOT_FOUND",
    }


def build_job_response(job: JobRecord, *, timed_out: bool = False) -> dict[str, Any]:
    is_terminal = job.status in {JobStatus.SUCCEEDED, JobStatus.FAILED, JobStatus.CANCELLED}

    result: dict[str, Any] = {
        "success": True,
        "job_id": job.job_id,
        "task_id": job.task_id,
        "action": job.action,
        "status": job.status.value,
        "created_at": job.created_at.isoformat(),
        "updated_at": job.updated_at.isoformat(),
        "message": job.message,
        "code": "JOB_TIMEOUT" if timed_out and not is_terminal else job.code,
        "timed_out": timed_out and not is_terminal,
    }
    if job.result is not None:
        result["result"] = job.result
    return result


def parse_timeout_seconds(value: object) -> float | None | str:
    if value is None:
        return None
    if isinstance(value, bool):
        return "timeout_seconds must be a non-negative number"
    if not isinstance(value, int | float):
        return "timeout_seconds must be a non-negative number"
    timeout = float(value)
    if timeout < 0:
        return "timeout_seconds must be >= 0"
    return timeout


def parse_events_limit(value: object) -> int | str:
    if value is None:
        return DEFAULT_EVENTS_LIMIT
    if isinstance(value, bool) or not isinstance(value, int):
        return f"limit must be an integer between 1 and {MAX_EVENTS_LIMIT}"
    if value < 1 or value > MAX_EVENTS_LIMIT:
        return f"limit must be an integer between 1 and {MAX_EVENTS_LIMIT}"
    return value


def parse_events_offset(value: object) -> int | str:
    if value is None:
        return 0
    if isinstance(value, bool) or not isinstance(value, int):
        return "offset must be an integer >= 0"
    if value < 0:
        return "offset must be an integer >= 0"
    return value
