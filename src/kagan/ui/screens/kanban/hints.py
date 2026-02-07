"""Keybinding hint generation for the Kanban screen."""

from __future__ import annotations

from kagan.core.models.enums import TaskStatus, TaskType
from kagan.keybindings import APP_BINDINGS, KANBAN_BINDINGS, get_key_for_action


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

    def key_for(action: str) -> str:
        key = get_key_for_action(KANBAN_BINDINGS, action, default="")
        if key:
            return key
        return get_key_for_action(APP_BINDINGS, action, default="?")

    def hint(action: str, label: str) -> tuple[str, str]:
        return (key_for(action), label)

    # No task selected - show general actions
    if status is None:
        return [
            hint("new_task", "new"),
            hint("toggle_search", "search"),
            hint("command_palette", "actions"),
        ]

    if status == TaskStatus.BACKLOG:
        return [
            hint("open_session", "start"),
            hint("edit_task", "edit"),
            hint("view_details", "details"),
            hint("toggle_peek", "peek"),
        ]

    if status == TaskStatus.IN_PROGRESS:
        if task_type == TaskType.AUTO:
            return [
                hint("watch_agent", "watch"),
                hint("stop_agent", "stop"),
                hint("toggle_peek", "peek"),
            ]
        return [
            hint("open_session", "open"),
            hint("view_details", "details"),
            hint("toggle_peek", "peek"),
        ]

    if status == TaskStatus.REVIEW:
        return [
            hint("open_review", "review"),
            hint("view_diff", "diff"),
            hint("merge_direct", "merge"),
        ]

    if status == TaskStatus.DONE:
        return [
            hint("view_details", "details"),
            hint("duplicate_task", "duplicate"),
            hint("delete_task_direct", "delete"),
        ]

    return []
