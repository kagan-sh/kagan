"""Shared service-layer type aliases."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, NewType, Protocol

if TYPE_CHECKING:
    from kagan.core.config import KaganConfig
    from kagan.core.models.enums import TaskStatus, TaskType

_ProjectId = NewType("_ProjectId", str)
_RepoId = NewType("_RepoId", str)
_TaskId = NewType("_TaskId", str)
_WorkspaceId = NewType("_WorkspaceId", str)
_SessionId = NewType("_SessionId", str)
_ExecutionId = NewType("_ExecutionId", str)
_MergeId = NewType("_MergeId", str)

type ProjectId = _ProjectId | str
type RepoId = _RepoId | str
type TaskId = _TaskId | str
type WorkspaceId = _WorkspaceId | str
type SessionId = _SessionId | str
type ExecutionId = _ExecutionId | str
type MergeId = _MergeId | str


class TaskLike(Protocol):
    """Protocol for task-like objects (domain or DB models)."""

    id: str
    title: str
    description: str
    status: TaskStatus
    task_type: TaskType
    agent_backend: str | None
    base_branch: str | None
    acceptance_criteria: list[str]

    def get_agent_config(self, config: KaganConfig) -> Any: ...
