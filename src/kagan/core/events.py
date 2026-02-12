"""Domain events and event bus contracts."""

from __future__ import annotations

from collections.abc import AsyncIterator, Callable
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Protocol
from uuid import uuid4

from kagan.core.time import utc_now

if TYPE_CHECKING:
    from datetime import datetime

    from kagan.core.models.enums import TaskStatus


def _new_event_id() -> str:
    return uuid4().hex


def _now() -> datetime:
    return utc_now()


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
class TaskDeleted:
    task_id: str
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
class AutomationTaskStarted:
    """Emitted when an AUTO task enters automation running state."""

    task_id: str
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class AutomationAgentAttached:
    """Emitted when a running AUTO task gets a live implementation agent."""

    task_id: str
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class AutomationReviewAgentAttached:
    """Emitted when a running AUTO task gets a live review agent."""

    task_id: str
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class AutomationTaskEnded:
    """Emitted when an AUTO task leaves automation running state."""

    task_id: str
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


@dataclass(frozen=True)
class CoreHostStarting:
    """Emitted when the core host begins startup."""

    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class CoreHostRunning:
    """Emitted when the core host is fully running and accepting connections."""

    transport: str
    address: str
    port: int | None = None
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class CoreHostDraining:
    """Emitted when the core host begins graceful shutdown."""

    reason: str
    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)


@dataclass(frozen=True)
class CoreHostStopped:
    """Emitted when the core host has fully stopped."""

    event_id: str = field(default_factory=_new_event_id)
    occurred_at: datetime = field(default_factory=_now)
