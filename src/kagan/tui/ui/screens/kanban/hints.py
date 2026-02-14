"""Keybinding hint generation for the Kanban screen."""

from __future__ import annotations

from dataclasses import dataclass, field

from kagan.core.models.enums import TaskStatus, TaskType
from kagan.tui.keybindings import APP_BINDINGS, KANBAN_BINDINGS, get_key_for_action


@dataclass
class KanbanHints:
    """Two-tier hint structure for the Kanban board."""

    navigation: list[tuple[str, str]] = field(default_factory=list)
    actions: list[tuple[str, str]] = field(default_factory=list)
    global_hints: list[tuple[str, str]] = field(default_factory=list)
    github_connected: bool = False


def _key_for(action: str) -> str:
    key = get_key_for_action(KANBAN_BINDINGS, action, default="")
    if key:
        return key
    return get_key_for_action(APP_BINDINGS, action, default="?")


def _hint(action: str, label: str) -> tuple[str, str]:
    return (_key_for(action), label)


def build_keybinding_hints(
    status: TaskStatus | None,
    task_type: TaskType | None,
    *,
    github_connected: bool = False,
) -> list[tuple[str, str]]:
    """Build context-sensitive keybinding hints based on task state.

    Legacy interface for non-Kanban screens. Returns a flat list.
    """
    return build_kanban_hints(status, task_type, github_connected=github_connected).actions


def build_kanban_hints(
    status: TaskStatus | None,
    task_type: TaskType | None,
    *,
    github_connected: bool = False,
) -> KanbanHints:
    """Build two-tier keybinding hints for the Kanban board.

    Args:
        status: Current task status, or None if no task is selected.
        task_type: Current task type.
        github_connected: Whether the repo is connected to GitHub.

    Returns:
        KanbanHints with navigation (row 1) and actions (row 2).
    """
    hints = KanbanHints(github_connected=github_connected)

    hints.global_hints = [
        _hint("switch_global_agent", "agent"),
        _hint("show_help", "help"),
        _hint("command_palette", "actions"),
    ]

    # Add GitHub sync hint if connected
    if github_connected:
        hints.global_hints.insert(0, _hint("github_sync", "sync"))

    if status is None:
        hints.navigation = []
        hints.actions = [
            _hint("new_task", "new"),
            _hint("new_auto_task", "new auto"),
            _hint("toggle_search", "search"),
            _hint("open_planner", "plan"),
        ]
        return hints

    # Navigation row: always show movement when a card is focused
    hints.navigation = [
        _hint("move_backward", "move left"),
        _hint("move_forward", "move right"),
    ]

    if status == TaskStatus.BACKLOG:
        hints.actions = [
            _hint("open_session", "start"),
            _hint("start_agent", "agent"),
            _hint("edit_task", "edit"),
            _hint("view_details", "details"),
            _hint("toggle_peek", "peek"),
        ]

    elif status == TaskStatus.IN_PROGRESS:
        hints.actions = [
            _hint("open_session", "open"),
            _hint("view_details", "details"),
            _hint("edit_task", "edit"),
            _hint("toggle_peek", "peek"),
        ]
        if task_type == TaskType.AUTO:
            hints.actions[1:1] = [
                _hint("stop_agent", "stop"),
                _hint("start_agent", "restart"),
            ]

    elif status == TaskStatus.REVIEW:
        hints.actions = [
            _hint("open_session", "open"),
            _hint("view_diff", "diff"),
            _hint("merge_direct", "merge"),
            _hint("rebase", "rebase"),
            _hint("view_details", "details"),
        ]

    elif status == TaskStatus.DONE:
        hints.actions = [
            _hint("open_session", "history"),
            _hint("view_details", "details"),
            _hint("duplicate_task", "duplicate"),
            _hint("delete_task_direct", "delete"),
        ]

    return hints
