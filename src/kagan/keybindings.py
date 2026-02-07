"""Keybindings for Kagan TUI application.

Simplified keybinding system using Textual's native Binding class directly.
"""

from __future__ import annotations

from textual.binding import Binding, BindingType

# =============================================================================
# App Bindings
# =============================================================================

APP_BINDINGS: list[BindingType] = [
    Binding("q", "quit", "Quit"),
    Binding("f1", "show_help", "Help", key_display="F1", priority=True),
    Binding("question_mark", "show_help", "", show=False, key_display="?"),
    Binding("ctrl+p", "command_palette", "Palette", show=False),
    Binding("f12", "toggle_debug_log", "Debug", show=False),
]

# =============================================================================
# Kanban Bindings
# =============================================================================

KANBAN_BINDINGS: list[BindingType] = [
    # Global
    Binding("q", "quit", "Quit", priority=True),
    # Primary actions
    Binding("n", "new_task", "New"),
    Binding("N", "new_auto_task", "New AUTO", key_display="Shift+N"),
    Binding("v", "view_details", "View"),
    Binding("e", "edit_task", "Edit"),
    Binding("enter", "open_session", "Open"),
    Binding("g", "activate_leader", "Go..."),
    Binding("slash", "toggle_search", "Search", key_display="/"),
    Binding("p", "open_planner", "Plan Mode"),
    Binding("comma", "open_settings", "Settings", key_display=","),
    Binding("x", "delete_task_direct", "Delete"),
    Binding("y", "duplicate_task", "Yank", show=False),
    Binding("c", "copy_task_id", "Copy ID", show=False),
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
    # Context-specific actions
    Binding("a", "start_agent", "Start agent"),
    Binding("s", "stop_agent", "Stop agent"),
    Binding("w", "watch_agent", "Watch agent", show=False),
    Binding("D", "view_diff", "Diff", show=False),
    Binding("r", "open_review", "Review", show=False),
    Binding("m", "merge_direct", "Merge"),
    # Peek
    Binding("space", "toggle_peek", "Peek"),
    Binding("f", "expand_description", "Expand"),
    Binding("f5", "expand_description", "Full Editor", key_display="F5"),
    # Utility
    Binding("escape", "deselect", "", show=False),
    Binding("ctrl+c", "interrupt", "", show=False),
]

# Leader key bindings (g+key sequences) - for help display and action mapping
KANBAN_LEADER_BINDINGS: list[BindingType] = [
    Binding("h", "move_backward", "Move←"),
    Binding("l", "move_forward", "Move→"),
    Binding("d", "view_diff", "Diff"),
    Binding("r", "open_review", "Review"),
    Binding("w", "watch_agent", "Watch"),
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
    Binding("y", "confirm", "Yes"),
    Binding("n", "cancel", "No"),
    Binding("escape", "cancel", "Cancel"),
]

DESCRIPTION_EDITOR_BINDINGS: list[BindingType] = [
    Binding("escape", "cancel", "Cancel"),
    Binding("ctrl+s", "save", "Save"),
]

DIFF_BINDINGS: list[BindingType] = [
    Binding("y", "copy", "Copy"),
    Binding("a", "approve", "Approve"),
    Binding("r", "reject", "Reject"),
    Binding("escape", "close", "Close"),
]

DUPLICATE_TASK_BINDINGS: list[BindingType] = [
    Binding("escape", "cancel", "Cancel"),
    Binding("ctrl+s", "create", "Create"),
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
    Binding("escape", "shelve", "Shelve"),
    Binding("enter", "retry", "Retry", priority=True),
    Binding(
        "ctrl+s", "stage", "Stage"
    ),  # Note: ctrl+enter may not work in all terminals, using ctrl+s
]

REVIEW_BINDINGS: list[BindingType] = [
    Binding("y", "copy", "Copy"),
    Binding("escape", "close_or_cancel", "Close/Cancel"),
    Binding("a", "approve", "Approve"),
    Binding("r", "reject", "Reject"),
    Binding("g", "generate_review", "AI Review"),
    Binding("s", "generate_review", "Start Review", show=False),
]

SETTINGS_BINDINGS: list[BindingType] = [
    Binding("escape", "cancel", "Cancel"),
    Binding("ctrl+s", "save", "Save"),
]

DEBUG_LOG_BINDINGS: list[BindingType] = [
    Binding("escape", "close", "Close"),
    Binding("c", "clear_logs", "Clear"),
    Binding("s", "save_logs", "Save"),
]

TASK_DETAILS_BINDINGS: list[BindingType] = [
    Binding("y", "copy", "Copy"),
    Binding("escape", "close_or_cancel", "Close/Cancel"),
    Binding("e", "toggle_edit", "Edit"),
    Binding("d", "delete", "Delete"),
    Binding("f", "expand_description", "Expand"),
    Binding("f5", "full_editor", "Full Editor", key_display="F5"),
    Binding("ctrl+s", "save", "Save", key_display="^s"),
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
    Binding("ctrl+e", "refine", "Enhance", priority=True),
]

TASK_EDITOR_BINDINGS: list[BindingType] = [
    Binding("escape", "cancel", "Cancel"),
    Binding("ctrl+s", "finish", "Finish Editing"),
]

TROUBLESHOOTING_BINDINGS: list[BindingType] = [
    Binding("q", "quit", "Quit"),
    Binding("escape", "quit", "Quit"),
    Binding("enter", "continue_app", "Continue", show=False),
    Binding("c", "continue_app", "Continue", show=False),
    Binding("i", "install_agent", "Install Agent", show=False),
]

WELCOME_BINDINGS: list[BindingType] = [
    Binding("escape", "skip", "Continue"),
]

# =============================================================================
# Widget Bindings
# =============================================================================

PERMISSION_PROMPT_BINDINGS: list[BindingType] = [
    Binding("y", "allow_once", "Allow once", show=False),
    Binding("a", "allow_always", "Allow always", show=False),
    Binding("n", "deny", "Deny", show=False),
    Binding("escape", "deny", "Deny", show=False),
]

# =============================================================================
# Utility Functions
# =============================================================================


def generate_leader_hint(bindings: list[BindingType]) -> str:
    """Generate the leader key hint string from leader bindings."""
    if not bindings:
        return ""

    parts = []
    for b in bindings:
        if isinstance(b, Binding):
            key = b.key_display or b.key
            desc = b.description.split()[0] if b.description else b.action
            parts.append(f"{key}={desc}")

    return " LEADER: " + " ".join(parts) + " | Esc=Cancel"


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
