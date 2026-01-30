"""Test helpers package."""

from tests.helpers.e2e import create_kagan_config_content, setup_kagan_dir
from tests.helpers.git import configure_git_user, init_git_repo_with_commit
from tests.helpers.mocks import (
    create_mock_agent,
    create_mock_process,
    create_mock_session_manager,
    create_mock_worktree_manager,
    create_test_config,
)
from tests.helpers.pages import (
    create_ticket_via_ui,
    delete_focused_ticket,
    focus_first_ticket,
    get_all_visible_tickets,
    get_focused_ticket,
    get_ticket_count,
    get_tickets_by_status,
    is_on_screen,
    move_ticket_backward,
    move_ticket_forward,
    navigate_to_kanban,
    skip_welcome_if_shown,
    toggle_ticket_type,
)

__all__ = [
    "configure_git_user",
    "create_kagan_config_content",
    "create_mock_agent",
    "create_mock_process",
    "create_mock_session_manager",
    "create_mock_worktree_manager",
    "create_test_config",
    "create_ticket_via_ui",
    "delete_focused_ticket",
    "focus_first_ticket",
    "get_all_visible_tickets",
    "get_focused_ticket",
    "get_ticket_count",
    "get_tickets_by_status",
    "init_git_repo_with_commit",
    "is_on_screen",
    "move_ticket_backward",
    "move_ticket_forward",
    "navigate_to_kanban",
    "setup_kagan_dir",
    "skip_welcome_if_shown",
    "toggle_ticket_type",
]
