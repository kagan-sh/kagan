"""Keybinding hint generation for the Kanban screen."""

from __future__ import annotations

from kagan.core.models.enums import TaskStatus, TaskType


def build_keybinding_hints(
    status: TaskStatus | None,
    task_type: TaskType | None,
) -> list[tuple[str, str]]:
    """Build context-sensitive keybinding hints based on task state.

    Args:
        status: Current task status, or None if no task selected.
        task_type: Task type (AUTO/PAIR), or None if no task selected.

    Returns:
        List of (key, description) tuples for display.
    """
    # No task selected - show general actions
    if status is None:
        return [
            ("n", "new task"),
            ("N", "new AUTO"),
            ("ctrl+p", "planner"),
            ("/", "search"),
            ("g", "more actions..."),
        ]

    if status == TaskStatus.BACKLOG:
        return [
            ("Enter", "start"),
            ("e", "edit"),
            ("v", "details"),
            ("x", "delete"),
            ("g", "more..."),
        ]

    if status == TaskStatus.IN_PROGRESS:
        if task_type == TaskType.AUTO:
            return [
                ("w", "watch agent"),
                ("s", "stop agent"),
                ("v", "details"),
                ("p", "peek status"),
                ("g", "more..."),
            ]
        return [
            ("a", "open session"),
            ("v", "details"),
            ("l", "advance"),
            ("g", "more..."),
        ]

    if status == TaskStatus.REVIEW:
        return [
            ("r", "AI review"),
            ("D", "view diff"),
            ("m", "merge"),
            ("h", "move back"),
            ("g", "more..."),
        ]

    if status == TaskStatus.DONE:
        return [
            ("v", "view details"),
            ("h", "reopen"),
            ("g", "more..."),
        ]

    return []
