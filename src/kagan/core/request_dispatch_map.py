"""Canonical capability.method -> request handler dispatch map."""

from __future__ import annotations

from collections.abc import Awaitable, Callable
from typing import Any

RequestHandler = Callable[[Any, dict[str, Any]], Awaitable[dict[str, Any]]]


def build_request_dispatch_map() -> dict[tuple[str, str], RequestHandler]:
    """Build the full dispatch map, importing handlers lazily to avoid cycles."""
    from kagan.core.request_handlers import (
        handle_audit_list,
        handle_diagnostics_instrumentation,
        handle_job_cancel,
        handle_job_events,
        handle_job_get,
        handle_job_submit,
        handle_job_wait,
        handle_project_add_repo,
        handle_project_create,
        handle_project_find_by_repo_path,
        handle_project_get,
        handle_project_list,
        handle_project_open,
        handle_project_repo_details,
        handle_project_repos,
        handle_review_approve,
        handle_review_merge,
        handle_review_rebase,
        handle_review_reject,
        handle_review_request,
        handle_session_attach,
        handle_session_create,
        handle_session_exists,
        handle_session_kill,
        handle_settings_get,
        handle_settings_update,
        handle_task_context,
        handle_task_create,
        handle_task_delete,
        handle_task_get,
        handle_task_list,
        handle_task_logs,
        handle_task_move,
        handle_task_scratchpad,
        handle_task_search,
        handle_task_update,
        handle_task_update_scratchpad,
        handle_task_wait,
    )

    return {
        # Tasks (12)
        ("tasks", "get"): handle_task_get,
        ("tasks", "list"): handle_task_list,
        ("tasks", "search"): handle_task_search,
        ("tasks", "scratchpad"): handle_task_scratchpad,
        ("tasks", "context"): handle_task_context,
        ("tasks", "logs"): handle_task_logs,
        ("tasks", "wait"): handle_task_wait,
        ("tasks", "create"): handle_task_create,
        ("tasks", "update"): handle_task_update,
        ("tasks", "move"): handle_task_move,
        ("tasks", "delete"): handle_task_delete,
        ("tasks", "update_scratchpad"): handle_task_update_scratchpad,
        # Review (5)
        ("review", "request"): handle_review_request,
        ("review", "approve"): handle_review_approve,
        ("review", "reject"): handle_review_reject,
        ("review", "merge"): handle_review_merge,
        ("review", "rebase"): handle_review_rebase,
        # Jobs (5)
        ("jobs", "submit"): handle_job_submit,
        ("jobs", "cancel"): handle_job_cancel,
        ("jobs", "get"): handle_job_get,
        ("jobs", "wait"): handle_job_wait,
        ("jobs", "events"): handle_job_events,
        # Sessions (4)
        ("sessions", "create"): handle_session_create,
        ("sessions", "attach"): handle_session_attach,
        ("sessions", "exists"): handle_session_exists,
        ("sessions", "kill"): handle_session_kill,
        # Projects (8)
        ("projects", "create"): handle_project_create,
        ("projects", "open"): handle_project_open,
        ("projects", "add_repo"): handle_project_add_repo,
        ("projects", "get"): handle_project_get,
        ("projects", "list"): handle_project_list,
        ("projects", "repos"): handle_project_repos,
        ("projects", "find_by_repo_path"): handle_project_find_by_repo_path,
        ("projects", "repo_details"): handle_project_repo_details,
        # Settings (2)
        ("settings", "get"): handle_settings_get,
        ("settings", "update"): handle_settings_update,
        # Audit (1)
        ("audit", "list"): handle_audit_list,
        # Diagnostics (1)
        ("diagnostics", "instrumentation"): handle_diagnostics_instrumentation,
    }


__all__ = ["RequestHandler", "build_request_dispatch_map"]
