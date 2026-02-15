"""Typed orchestration API for all Kagan operations.

KaganAPI wraps AppContext and exposes direct method calls
instead of stringly-typed (capability, method) dispatch. It sits at
the same level as command/query handlers and delegates to the
underlying services for each operation.

The implementation is split across mixin modules for maintainability:
- ``api_tasks.py``      — task CRUD, scratchpad, context, logs, reviews
- ``api_projects.py``   — project/repo management, settings, audit
- ``api_automation.py`` — jobs, sessions, runtime, agents, workspaces, merges
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from kagan.core.api_automation import AutomationApiMixin
from kagan.core.api_github import GitHubApiMixin
from kagan.core.api_plugin_ui import PluginUiApiMixin
from kagan.core.api_projects import ProjectApiMixin
from kagan.core.api_tasks import TaskApiMixin
from kagan.core.expose import expose
from kagan.core.instrumentation import snapshot as instrumentation_snapshot

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.core.adapters.db.schema import Task
    from kagan.core.bootstrap import AppContext


# ── Error classes ──────────────────────────────────────────────────────


class SessionError(ValueError):
    """Base for session-related domain errors with a machine-readable code."""

    def __init__(self, message: str, *, code: str, task_id: str) -> None:
        super().__init__(message)
        self.code = code
        self.task_id = task_id


class TaskNotFoundError(SessionError):
    """Raised when the target task does not exist."""

    def __init__(self, task_id: str) -> None:
        super().__init__(f"Task {task_id} not found", code="TASK_NOT_FOUND", task_id=task_id)


class TaskTypeMismatchError(SessionError):
    """Raised when a non-PAIR task is used for session ops."""

    def __init__(self, task_id: str, current_task_type: str) -> None:
        super().__init__(
            "Only PAIR tasks support interactive session handoff",
            code="TASK_TYPE_MISMATCH",
            task_id=task_id,
        )
        self.current_task_type = current_task_type


class WorkspaceNotFoundError(SessionError):
    """Raised when no workspace exists for the task."""

    def __init__(self, task_id: str) -> None:
        super().__init__(
            f"No workspace found for task {task_id}",
            code="WORKSPACE_NOT_FOUND",
            task_id=task_id,
        )


class InvalidWorktreePathError(SessionError):
    """Raised when a provided worktree_path is invalid or mismatched."""

    def __init__(self, task_id: str, message: str) -> None:
        super().__init__(message, code="INVALID_WORKTREE_PATH", task_id=task_id)


class SessionCreateFailedError(SessionError):
    """Raised when the underlying session backend fails."""

    def __init__(self, task_id: str, cause: Exception) -> None:
        super().__init__(
            f"Failed to create session: {cause}",
            code="SESSION_CREATE_FAILED",
            task_id=task_id,
        )
        self.__cause__ = cause


# ── Dataclasses ────────────────────────────────────────────────────────


@dataclass(frozen=True, slots=True)
class SessionCreateResult:
    """Rich result from api.create_session()."""

    session_name: str
    already_exists: bool
    worktree_path: Path
    task: Task


# ── API ─────────────────────────────────────────────────────────────


class KaganAPI(TaskApiMixin, ProjectApiMixin, AutomationApiMixin, GitHubApiMixin, PluginUiApiMixin):
    """Typed orchestration API for all Kagan operations.

    Wraps the existing AppContext and provides direct method calls
    instead of stringly-typed (capability, method) dispatch.
    """

    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    @property
    def ctx(self) -> AppContext:
        """Access to underlying AppContext for gradual migration."""
        return self._ctx

    @expose(
        "diagnostics",
        "instrumentation",
        profile="maintainer",
        description="Return in-memory instrumentation aggregates.",
    )
    async def get_instrumentation(self) -> dict[str, Any]:
        """Return in-memory instrumentation aggregates.

        Overrides the mixin to use the module-level ``instrumentation_snapshot``
        reference, preserving monkeypatch-ability on ``kagan.core.api``.
        """
        return instrumentation_snapshot()


__all__ = [
    "InvalidWorktreePathError",
    "KaganAPI",
    "SessionCreateFailedError",
    "SessionCreateResult",
    "SessionError",
    "TaskNotFoundError",
    "TaskTypeMismatchError",
    "WorkspaceNotFoundError",
]
