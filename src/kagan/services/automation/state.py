from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import asyncio

    from kagan.core.models.enums import ExecutionKind, TaskStatus


@dataclass(slots=True)
class RunningTaskState:
    """Lifecycle state for a currently running task loop."""

    task: asyncio.Task[None] | None = None
    session_id: str | None = None
    pending_respawn: bool = False


@dataclass(slots=True)
class AutomationEvent:
    """Queue item for automation worker."""

    kind: ExecutionKind
    task_id: str
    old_status: TaskStatus | None = None
    new_status: TaskStatus | None = None
