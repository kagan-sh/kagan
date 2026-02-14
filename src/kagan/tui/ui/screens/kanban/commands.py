"""Kanban command palette provider."""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum
from functools import partial

from textual.command import DiscoveryHit, Hit, Hits, Provider

from kagan.tui.ui.screens.command_actions import run_screen_action, screen_allows_action


class KanbanActionId(StrEnum):
    NEW_TASK = "new_task"
    NEW_AUTO_TASK = "new_auto_task"
    OPEN_SESSION = "open_session"
    EDIT_TASK = "edit_task"
    VIEW_DETAILS = "view_details"
    DELETE_TASK_DIRECT = "delete_task_direct"
    DUPLICATE_TASK = "duplicate_task"
    TOGGLE_PEEK = "toggle_peek"
    MOVE_BACKWARD = "move_backward"
    MOVE_FORWARD = "move_forward"
    START_AGENT = "start_agent"
    STOP_AGENT = "stop_agent"
    VIEW_DIFF = "view_diff"
    OPEN_REVIEW = "open_review"
    MERGE_DIRECT = "merge_direct"
    REBASE = "rebase"
    TOGGLE_SEARCH = "toggle_search"
    OPEN_PLANNER = "open_planner"
    SWITCH_GLOBAL_AGENT = "switch_global_agent"
    OPEN_SETTINGS = "open_settings"
    SET_TASK_BRANCH = "set_task_branch"
    MERGE = "merge"
    GITHUB_SYNC = "github_sync"


@dataclass(frozen=True, slots=True)
class KanbanAction:
    command: str
    help: str
    action: KanbanActionId
    requires_task: bool = False
    requires_agent: bool = False
    worker_group: str | None = None
    exclusive: bool = False
    exit_on_error: bool = False


KANBAN_ACTIONS: tuple[KanbanAction, ...] = (
    KanbanAction(
        "task new",
        "Create a new task",
        KanbanActionId.NEW_TASK,
        worker_group="task-modal-new",
    ),
    KanbanAction(
        "task new auto",
        "Create a new AUTO task",
        KanbanActionId.NEW_AUTO_TASK,
        worker_group="task-modal-new-auto",
    ),
    KanbanAction(
        "task open",
        "Open session or start task",
        KanbanActionId.OPEN_SESSION,
        requires_task=True,
        worker_group="open-session",
    ),
    KanbanAction(
        "task edit",
        "Edit selected task",
        KanbanActionId.EDIT_TASK,
        requires_task=True,
        worker_group="task-modal-edit",
    ),
    KanbanAction(
        "task details",
        "View task details",
        KanbanActionId.VIEW_DETAILS,
        requires_task=True,
        worker_group="task-modal-view",
    ),
    KanbanAction(
        "task delete",
        "Delete selected task",
        KanbanActionId.DELETE_TASK_DIRECT,
        requires_task=True,
        worker_group="delete-task",
    ),
    KanbanAction(
        "task duplicate",
        "Duplicate selected task",
        KanbanActionId.DUPLICATE_TASK,
        requires_task=True,
        worker_group="duplicate-task",
    ),
    KanbanAction(
        "task peek",
        "Toggle peek overlay",
        KanbanActionId.TOGGLE_PEEK,
        requires_task=True,
    ),
    KanbanAction(
        "task move left",
        "Move task to previous column",
        KanbanActionId.MOVE_BACKWARD,
        requires_task=True,
        worker_group="review-move-backward",
    ),
    KanbanAction(
        "task move right",
        "Move task to next column",
        KanbanActionId.MOVE_FORWARD,
        requires_task=True,
        worker_group="review-move-forward",
    ),
    KanbanAction(
        "task start agent",
        "Start AUTO agent",
        KanbanActionId.START_AGENT,
        requires_task=True,
        requires_agent=True,
        worker_group="start-agent",
        exclusive=True,
    ),
    KanbanAction(
        "task stop agent",
        "Stop AUTO agent",
        KanbanActionId.STOP_AGENT,
        requires_task=True,
        worker_group="stop-agent",
        exclusive=True,
    ),
    KanbanAction(
        "task diff",
        "View diff for REVIEW tasks",
        KanbanActionId.VIEW_DIFF,
        requires_task=True,
        worker_group="review-view-diff",
    ),
    KanbanAction(
        "task review",
        "Open review modal",
        KanbanActionId.OPEN_REVIEW,
        requires_task=True,
        worker_group="review-open",
    ),
    KanbanAction(
        "task merge",
        "Merge task",
        KanbanActionId.MERGE_DIRECT,
        requires_task=True,
        worker_group="review-merge-direct",
    ),
    KanbanAction(
        "task rebase",
        "Rebase task branch onto base",
        KanbanActionId.REBASE,
        requires_task=True,
        worker_group="review-rebase",
    ),
    KanbanAction("board search", "Toggle search bar", KanbanActionId.TOGGLE_SEARCH),
    KanbanAction(
        "board plan mode",
        "Open planner",
        KanbanActionId.OPEN_PLANNER,
        requires_agent=True,
    ),
    KanbanAction(
        "board switch agent",
        "Pick default global agent",
        KanbanActionId.SWITCH_GLOBAL_AGENT,
        worker_group="switch-global-agent",
        exclusive=True,
        exit_on_error=False,
    ),
    KanbanAction(
        "board settings",
        "Open settings",
        KanbanActionId.OPEN_SETTINGS,
        worker_group="open-settings",
        exclusive=True,
        exit_on_error=False,
    ),
    KanbanAction(
        "board set task branch",
        "Set base branch for focused task",
        KanbanActionId.SET_TASK_BRANCH,
        requires_task=True,
        worker_group="set-task-branch",
        exclusive=True,
        exit_on_error=False,
    ),
    KanbanAction(
        "github sync",
        "Sync issues from connected GitHub repo",
        KanbanActionId.GITHUB_SYNC,
        worker_group="github-sync",
        exclusive=True,
        exit_on_error=False,
    ),
)


def get_kanban_action(action: str | KanbanActionId) -> KanbanAction | None:
    for item in KANBAN_ACTIONS:
        if item.action == action:
            return item
    return None


class KanbanCommandProvider(Provider):
    """Command palette provider for Kanban actions."""

    async def search(self, query: str) -> Hits:
        screen = self.screen
        matcher = self.matcher(query)
        for item in KANBAN_ACTIONS:
            if not screen_allows_action(
                screen,
                item.action.value,
                dispatch_method="run_kanban_action",
            ):
                continue
            score = matcher.match(item.command)
            if score <= 0:
                continue
            yield Hit(
                score,
                matcher.highlight(item.command),
                partial(
                    run_screen_action,
                    screen,
                    item.action.value,
                    dispatch_method="run_kanban_action",
                ),
                help=item.help,
            )

    async def discover(self) -> Hits:
        screen = self.screen
        for item in KANBAN_ACTIONS:
            if not screen_allows_action(
                screen,
                item.action.value,
                dispatch_method="run_kanban_action",
            ):
                continue
            yield DiscoveryHit(
                item.command,
                partial(
                    run_screen_action,
                    screen,
                    item.action.value,
                    dispatch_method="run_kanban_action",
                ),
                help=item.help,
            )
