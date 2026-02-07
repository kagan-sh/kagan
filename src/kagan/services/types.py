"""Shared service-layer type aliases."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Protocol

if TYPE_CHECKING:
    from kagan.config import KaganConfig
    from kagan.core.models.enums import TaskStatus, TaskType

type ProjectId = str
type RepoId = str
type TaskId = str
type WorkspaceId = str
type SessionId = str
type ExecutionId = str
type MergeId = str


class TaskLike(Protocol):
    """Protocol for task-like objects (domain or DB models)."""

    id: str
    title: str
    description: str
    status: TaskStatus
    task_type: TaskType
    assigned_hat: str | None
    agent_backend: str | None
    base_branch: str | None
    acceptance_criteria: list[str]

    def get_agent_config(self, config: KaganConfig) -> Any: ...
