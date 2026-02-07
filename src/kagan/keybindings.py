"""Keybindings for Kagan TUI application.

Simplified keybinding system using Textual's native Binding class directly.
"""

from __future__ import annotations

from textual.binding import Binding, BindingType

# =============================================================================
# App Bindings
# =============================================================================

APP_BINDINGS: list[BindingType] = [
    Binding(".", "command_palette", "Actions", key_display="."),
    Binding("question_mark", "show_help", "Help", key_display="?"),
    Binding("f1", "show_help", "", show=False, key_display="F1", priority=True),
    Binding("q", "quit", "Quit", show=False),
    Binding(
        "ctrl+o",
        "open_project_selector",
        "Projects",
        key_display="Ctrl+O",
        show=False,
        priority=True,
    ),
    Binding(
        "ctrl+r",
        "open_repo_selector",
        "Repos",
        key_display="Ctrl+R",
        show=False,
        priority=True,
    ),
    Binding("ctrl+p", "command_palette", "", show=False),
    Binding("f12", "toggle_debug_log", "Debug", show=False),
]

# =============================================================================
# Kanban Bindings
# =============================================================================

KANBAN_BINDINGS: list[BindingType] = [
    # Primary actions
    Binding("n", "new_task", "New"),
    Binding("enter", "open_session", "Open", key_display="Enter"),
    Binding("slash", "toggle_search", "Search", key_display="/"),
    # Task actions (hidden by default, surfaced via context hints / palette)
    Binding("N", "new_auto_task", "New AUTO", key_display="Shift+N", show=False),
    Binding("v", "view_details", "View", show=False),
    Binding("e", "edit_task", "Edit", show=False),
    Binding("x", "delete_task_direct", "Delete", show=False),
    Binding("y", "duplicate_task", "Duplicate", show=False),
    Binding("c", "copy_task_id", "Copy ID", show=False),
    Binding("space", "toggle_peek", "Peek", show=False),
    Binding("f", "expand_description", "Expand", show=False),
    Binding("f5", "expand_description", "Full Editor", key_display="F5", show=False),
    # Move between columns (power)
    Binding("H", "move_backward", "Move Left", key_display="Shift+H", show=False),
    Binding("L", "move_forward", "Move Right", key_display="Shift+L", show=False),
    # Context-specific actions
    Binding("a", "start_agent", "Start agent", show=False),
    Binding("s", "stop_agent", "Stop agent", show=False),
    Binding("w", "watch_agent", "Watch agent", show=False),
    Binding("D", "view_diff", "Diff", key_display="Shift+D", show=False),
    Binding("r", "open_review", "Review", show=False),
    Binding("m", "merge_direct", "Merge", show=False),
    Binding("p", "open_planner", "Plan Mode", show=False),
    Binding("comma", "open_settings", "Settings", key_display=",", show=False),
    # Navigation - vim style
    Binding("h", "focus_left", "Left", show=False),
    Binding("j", "focus_down", "Down", show=False),
    Binding("k", "focus_up", "Up", show=False),
    Binding("l", "focus_right", "Right", show=False),
    # Navigation - arrow keys
    Binding("left", "focus_left", "Left", show=False),
    Binding("right", "focus_right", "Right", show=False),
    Binding("down", "focus_down", "Down", show=False),
    Binding("up", "focus_up", "Up", show=False),
    # Navigation - tab
    Binding("tab", "focus_right", "Next Column", show=False),
    Binding("shift+tab", "focus_left", "Prev Column", show=False),
    # Utility
    Binding("escape", "deselect", "", show=False),
    Binding("ctrl+c", "interrupt", "", show=False),
]

# =============================================================================
# Modal Bindings
# =============================================================================

AGENT_OUTPUT_BINDINGS: list[BindingType] = [
    Binding("y", "copy", "Copy"),
    Binding("escape", "close", "Close"),
    Binding("c", "cancel_agent", "Cancel Agent"),
]

CONFIRM_BINDINGS: list[BindingType] = [
    Binding("enter", "confirm", "Confirm", key_display="Enter"),
    Binding("escape", "cancel", "Cancel"),
    Binding("y", "confirm", "Yes", show=False),
    Binding("n", "cancel", "No", show=False),
]

DESCRIPTION_EDITOR_BINDINGS: list[BindingType] = [
    Binding("escape", "cancel", "Cancel"),
    Binding("f2", "save", "Save", key_display="F2"),
    Binding("alt+s", "save", "Save", show=False),
]

DIFF_BINDINGS: list[BindingType] = [
    Binding("enter", "approve", "Approve", key_display="Enter"),
    Binding("r", "reject", "Reject"),
    Binding("y", "copy", "Copy", show=False),
    Binding("escape", "close", "Close"),
]

DUPLICATE_TASK_BINDINGS: list[BindingType] = [
    Binding("enter", "create", "Create", key_display="Enter"),
    Binding("escape", "cancel", "Cancel"),
]

HELP_BINDINGS: list[BindingType] = [
    Binding("escape", "close", "Close"),
    Binding("q", "close", "Close", show=False),
]

INSTALL_MODAL_BINDINGS: list[BindingType] = [
    Binding("enter", "confirm", "Install"),
    Binding("escape", "cancel", "Cancel"),
]

REJECTION_INPUT_BINDINGS: list[BindingType] = [
    Binding("enter", "retry", "Retry", key_display="Enter", priority=True),
    Binding("f2", "stage", "Stage", key_display="F2"),
    Binding("escape", "shelve", "Shelve"),
]

REVIEW_BINDINGS: list[BindingType] = [
    Binding("enter", "approve", "Approve", key_display="Enter"),
    Binding("r", "reject", "Reject"),
    Binding("g", "generate_review", "AI Review", show=False),
    Binding("y", "copy", "Copy", show=False),
    Binding("escape", "close_or_cancel", "Close/Cancel"),
]

SETTINGS_BINDINGS: list[BindingType] = [
    Binding("escape", "cancel", "Cancel"),
    Binding("f2", "save", "Save", key_display="F2"),
    Binding("alt+s", "save", "Save", show=False),
]

DEBUG_LOG_BINDINGS: list[BindingType] = [
    Binding("escape", "close", "Close"),
    Binding("c", "clear_logs", "Clear"),
    Binding("s", "save_logs", "Save"),
]

TASK_DETAILS_BINDINGS: list[BindingType] = [
    Binding("escape", "close_or_cancel", "Close/Cancel"),
    Binding("e", "toggle_edit", "Edit"),
    Binding("d", "delete", "Delete"),
    Binding("f", "expand_description", "Expand"),
    Binding("f5", "full_editor", "Full Editor", key_display="F5"),
    Binding("f2", "save", "Save", key_display="F2"),
    Binding("alt+s", "save", "Save", show=False),
    Binding("y", "copy", "Copy", show=False),
]

TMUX_GATEWAY_BINDINGS: list[BindingType] = [
    Binding("enter", "proceed", "Continue", priority=True),
    Binding("escape", "cancel", "Cancel", priority=True),
    Binding("s", "skip_future", "Don't show again", priority=True),
]

# =============================================================================
# Screen Bindings
# =============================================================================

APPROVAL_BINDINGS: list[BindingType] = [
    Binding("escape", "cancel", "Cancel"),
    Binding("enter", "approve", "Approve"),
    Binding("t", "toggle_type", "Toggle Type"),
]

PLANNER_BINDINGS: list[BindingType] = [
    Binding("escape", "to_board", "Go to Board"),
    Binding("ctrl+c", "cancel", "Stop", priority=True),
    Binding("f2", "refine", "Enhance", key_display="F2", priority=True),
]

TASK_EDITOR_BINDINGS: list[BindingType] = [
    Binding("escape", "cancel", "Cancel"),
    Binding("f2", "finish", "Finish Editing", key_display="F2"),
    Binding("alt+s", "finish", "Finish Editing", show=False),
]

TROUBLESHOOTING_BINDINGS: list[BindingType] = [
    Binding("q", "quit", "Quit"),
    Binding("escape", "quit", "Quit"),
    Binding("enter", "continue_app", "Continue", show=False),
    Binding("c", "continue_app", "Continue", show=False),
    Binding("i", "install_agent", "Install Agent", show=False),
]

WELCOME_BINDINGS: list[BindingType] = [
    Binding("n", "new_project", "New Project", show=False),
    Binding("o", "open_folder", "Open Folder", show=False),
    Binding("s", "settings", "Settings", show=False),
    Binding("enter", "open_selected", "Open", key_display="Enter", show=False),
    Binding("escape", "quit", "Quit", show=False),
    # Number shortcuts for quick project selection (1-9)
    Binding("1", "open_project_1", "Open #1", show=False),
    Binding("2", "open_project_2", "Open #2", show=False),
    Binding("3", "open_project_3", "Open #3", show=False),
    Binding("4", "open_project_4", "Open #4", show=False),
    Binding("5", "open_project_5", "Open #5", show=False),
    Binding("6", "open_project_6", "Open #6", show=False),
    Binding("7", "open_project_7", "Open #7", show=False),
    Binding("8", "open_project_8", "Open #8", show=False),
    Binding("9", "open_project_9", "Open #9", show=False),
]

ONBOARDING_BINDINGS: list[BindingType] = [
    Binding("escape", "quit", "Quit"),
]

# =============================================================================
# Widget Bindings
# =============================================================================

PERMISSION_PROMPT_BINDINGS: list[BindingType] = [
    Binding("enter", "allow_once", "Allow once", key_display="Enter"),
    Binding("a", "allow_always", "Allow always", show=False),
    Binding("escape", "deny", "Deny"),
    Binding("n", "deny", "Deny", show=False),
]

# =============================================================================
# Utility Functions
# =============================================================================


def get_key_for_action(bindings: list[BindingType], action: str, default: str = "?") -> str:
    """Get the display key for an action.

    Args:
        bindings: List of bindings to search
        action: The action name to find
        default: Value to return if action not found (default: "?")

    Returns:
        The display key (using key_display if set, else raw key), or default if not found
    """
    for b in bindings:
        if isinstance(b, Binding) and b.action == action:
            return b.key_display or b.key
    return default
