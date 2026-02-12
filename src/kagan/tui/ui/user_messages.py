"""Shared user-facing copy for common TUI messaging."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal


@dataclass(frozen=True, slots=True)
class InstanceLockCopy:
    """UI copy for repository lock messaging."""

    title: str
    message: str
    note: str
    button_label: str
    button_variant: Literal["default", "error", "primary", "success", "warning"]


def instance_lock_copy(*, is_startup: bool) -> InstanceLockCopy:
    """Return lock modal copy for startup and repo-switch scenarios."""
    if is_startup:
        return InstanceLockCopy(
            title="Repository Locked",
            message="Another Kagan instance is already running in this repository.",
            note="Close the other instance first, then start Kagan again.",
            button_label="Quit",
            button_variant="error",
        )
    return InstanceLockCopy(
        title="Repository Locked",
        message="Cannot switch to this repository because another instance holds the lock.",
        note="Close the other instance first, or continue in your current repository.",
        button_label="OK",
        button_variant="primary",
    )


PERMISSION_HEADER = "Permission required"
PERMISSION_ACTION_HINT = "Enter allow once · A allow always · Esc deny"


def permission_tool_line(title: str) -> str:
    """Format permission prompt tool context."""
    return f"Tool: {title}"


def permission_timer_line(remaining_seconds: int) -> str:
    """Format countdown copy for permission prompts."""
    minutes, seconds = divmod(max(remaining_seconds, 0), 60)
    return f"Waiting for decision... ({minutes}:{seconds:02d})"


def task_deleted_close_message(surface: str) -> str:
    """Format deletion message when a modal must close for stale task state."""
    return f"Task was deleted by another action. Closing {surface}."


def task_moved_close_message(status_value: str) -> str:
    """Format status-transition message when review/output modal closes."""
    return f"Task moved to {status_value.upper()}. Closing task output."
