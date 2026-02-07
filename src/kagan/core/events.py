"""Domain events and event bus contracts."""

from __future__ import annotations

from collections.abc import AsyncIterator, Callable
from dataclasses import dataclass, field
from datetime import datetime
from typing import TYPE_CHECKING, Protocol
from uuid import uuid4

if TYPE_CHECKING:
    from kagan.core.models.enums import TaskStatus


def _new_event_id() -> str:
    return uuid4().hex


def _now() -> datetime:
    return datetime.now()


class DomainEvent(Protocol):
    """Base protocol for all domain events."""

    @property
    def event_id(self) -> str: ...

    @property
    def occurred_at(self) -> datetime: ...


EventHandler = Callable[[DomainEvent], None]


class EventBus(Protocol):
    """Async fan-out bus for domain events."""

    async def publish(self, event: DomainEvent) -> None:
        """Publish a single event to subscribers."""
        ...

    def subscribe(self, event_type: type[DomainEvent] | None = None) -> AsyncIterator[DomainEvent]:
        """Subscribe to events (optionally filtered by type)."""
        ...

    def add_handler(
        self,
        handler: EventHandler,
        event_type: type[DomainEvent] | None = None,
    ) -> None:
        """Register a sync handler for events (UI bridges use this)."""
        ...

    def remove_handler(self, handler: EventHandler) -> None:
        """Remove a previously registered handler."""
        ...


@dataclass(frozen=True)
class TaskCreated:
    task_id: str
    status: TaskStatus
    title: str
    created_at: datetime
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class TaskUpdated:
    task_id: str
    fields_changed: list[str]
    updated_at: datetime
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class TaskStatusChanged:
    task_id: str
    from_status: TaskStatus
    to_status: TaskStatus
    reason: str | None
    updated_at: datetime
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class WorkspaceProvisioned:
    workspace_id: str
    task_id: str | None
    branch: str
    path: str
    repo_count: int
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class WorkspaceReleased:
    workspace_id: str
    task_id: str | None
    reason: str | None
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class SessionStarted:
    session_id: str
    workspace_id: str
    kind: str
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class SessionEnded:
    session_id: str
    workspace_id: str
    reason: str | None
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class ExecutionRequested:
    execution_id: str
    workspace_id: str | None
    kind: str
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class ExecutionStarted:
    execution_id: str
    workspace_id: str | None
    started_at: datetime
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class ExecutionOutputChunk:
    execution_id: str
    stream: str
    chunk: str
    seq: int
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class ExecutionCompleted:
    execution_id: str
    exit_code: int
    duration_ms: int
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class ExecutionFailed:
    execution_id: str
    error: str
    duration_ms: int
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class ExecutionCancelled:
    execution_id: str
    reason: str | None
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class MergeRequested:
    merge_id: str
    task_id: str
    workspace_id: str | None
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class MergeCompleted:
    """Emitted when a repo merge completes."""

    workspace_id: str
    repo_id: str
    target_branch: str
    commit_sha: str
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class MergeFailed:
    """Emitted when a repo merge fails."""

    workspace_id: str
    repo_id: str
    error: str
    conflict_op: str | None = None
    conflict_files: list[str] | None = None
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class WorkspaceRepoStatus:
    """Emitted when a workspace repo's status changes."""

    workspace_id: str
    repo_id: str
    has_changes: bool
    diff_stats: dict | None = None
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class ProjectOpened:
    """Emitted when a project is opened."""

    project_id: str
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class ProjectCreated:
    """Emitted when a new project is created."""

    project_id: str
    name: str
    repo_count: int
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class PRCreated:
    """Emitted when a PR is created for a repo."""

    workspace_id: str
    repo_id: str
    pr_url: str
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class ScriptCompleted:
    """Emitted when a repo script completes."""

    workspace_id: str
    repo_id: str
    script_type: str
    success: bool
    exit_code: int
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)
