"""Pure policy helpers for automation runner decisions."""

from __future__ import annotations

from kagan.core.models.enums import TaskStatus, TaskType


def is_auto_task(task_type: TaskType) -> bool:
    """Return whether a task type is eligible for automation."""
    return task_type is TaskType.AUTO


def should_stop_running_on_status_change(
    *,
    old_status: TaskStatus | None,
    new_status: TaskStatus | None,
) -> bool:
    """Return whether a running AUTO task should be stopped on status transition."""
    # Don't stop for REVIEW transitions as that's part of normal completion flow.
    return old_status is TaskStatus.IN_PROGRESS and new_status is not TaskStatus.REVIEW


def can_spawn_new_agent(*, running_count: int, max_agents: int) -> bool:
    """Return whether runner has capacity for another AUTO task."""
    return running_count < max_agents


__all__ = [
    "can_spawn_new_agent",
    "is_auto_task",
    "should_stop_running_on_status_change",
]
