"""Kanban command palette provider."""

from __future__ import annotations

import asyncio
from dataclasses import dataclass
from functools import partial
from typing import TYPE_CHECKING

from textual.command import DiscoveryHit, Hit, Hits, Provider

if TYPE_CHECKING:
    from textual.screen import Screen


@dataclass(frozen=True, slots=True)
class KanbanAction:
    command: str
    help: str
    action: str
    requires_task: bool = False
    requires_agent: bool = False
    worker_group: str | None = None
    exclusive: bool = False
    exit_on_error: bool = False


KANBAN_ACTIONS: tuple[KanbanAction, ...] = (
    KanbanAction("task new", "Create a new task", "new_task", worker_group="task-modal-new"),
    KanbanAction(
        "task new auto",
        "Create a new AUTO task",
        "new_auto_task",
        worker_group="task-modal-new-auto",
    ),
    KanbanAction(
        "task open",
        "Open session or start task",
        "open_session",
        requires_task=True,
        worker_group="open-session",
    ),
    KanbanAction(
        "task edit",
        "Edit selected task",
        "edit_task",
        requires_task=True,
        worker_group="task-modal-edit",
    ),
    KanbanAction(
        "task details",
        "View task details",
        "view_details",
        requires_task=True,
        worker_group="task-modal-view",
    ),
    KanbanAction(
        "task delete",
        "Delete selected task",
        "delete_task_direct",
        requires_task=True,
        worker_group="delete-task",
    ),
    KanbanAction(
        "task duplicate",
        "Duplicate selected task",
        "duplicate_task",
        requires_task=True,
        worker_group="duplicate-task",
    ),
    KanbanAction("task peek", "Toggle peek overlay", "toggle_peek", requires_task=True),
    KanbanAction(
        "task move left",
        "Move task to previous column",
        "move_backward",
        requires_task=True,
        worker_group="review-move-backward",
    ),
    KanbanAction(
        "task move right",
        "Move task to next column",
        "move_forward",
        requires_task=True,
        worker_group="review-move-forward",
    ),
    KanbanAction(
        "task start agent",
        "Start AUTO agent",
        "start_agent",
        requires_task=True,
        requires_agent=True,
        worker_group="start-agent",
    ),
    KanbanAction("task stop agent", "Stop AUTO agent", "stop_agent", requires_task=True),
    KanbanAction(
        "task diff",
        "View diff for REVIEW tasks",
        "view_diff",
        requires_task=True,
        worker_group="review-view-diff",
    ),
    KanbanAction(
        "task review",
        "Open review modal",
        "open_review",
        requires_task=True,
        worker_group="review-open",
    ),
    KanbanAction(
        "task merge",
        "Merge task",
        "merge_direct",
        requires_task=True,
        worker_group="review-merge-direct",
    ),
    KanbanAction(
        "task rebase",
        "Rebase task branch onto base",
        "rebase",
        requires_task=True,
        worker_group="review-rebase",
    ),
    KanbanAction("board search", "Toggle search bar", "toggle_search"),
    KanbanAction(
        "board plan mode",
        "Open planner",
        "open_planner",
        requires_agent=True,
    ),
    KanbanAction(
        "board switch agent",
        "Pick default global agent",
        "switch_global_agent",
        worker_group="switch-global-agent",
        exclusive=True,
        exit_on_error=False,
    ),
    KanbanAction(
        "board settings",
        "Open settings",
        "open_settings",
        worker_group="open-settings",
        exclusive=True,
        exit_on_error=False,
    ),
    KanbanAction(
        "board set task branch",
        "Set base branch for focused task",
        "set_task_branch",
        requires_task=True,
        worker_group="set-task-branch",
        exclusive=True,
        exit_on_error=False,
    ),
    KanbanAction(
        "board set default branch",
        "Set global default base branch",
        "set_default_branch",
        worker_group="set-default-branch",
        exclusive=True,
        exit_on_error=False,
    ),
)


def get_kanban_action(action: str) -> KanbanAction | None:
    for item in KANBAN_ACTIONS:
        if item.action == action:
            return item
    return None


def _screen_allows_action(screen: Screen, action: str) -> bool:
    run_kanban_action = getattr(screen, "run_kanban_action", None)
    if not callable(run_kanban_action) and not hasattr(screen, f"action_{action}"):
        return False
    check_action = getattr(screen, "check_action", None)
    if check_action is None:
        return True
    try:
        return check_action(action, ()) is True
    except Exception:
        return False


async def _run_screen_action(screen: Screen, action: str) -> None:
    run_kanban_action = getattr(screen, "run_kanban_action", None)
    if callable(run_kanban_action):
        result = run_kanban_action(action)
        if asyncio.iscoroutine(result):
            await result
        return

    action_method = getattr(screen, f"action_{action}", None)
    if action_method is None:
        return
    result = action_method()
    if asyncio.iscoroutine(result):
        await result


class KanbanCommandProvider(Provider):
    """Command palette provider for Kanban actions."""

    async def search(self, query: str) -> Hits:
        screen = self.screen
        matcher = self.matcher(query)
        for item in KANBAN_ACTIONS:
            if not _screen_allows_action(screen, item.action):
                continue
            score = matcher.match(item.command)
            if score <= 0:
                continue
            yield Hit(
                score,
                matcher.highlight(item.command),
                partial(_run_screen_action, screen, item.action),
                help=item.help,
            )

    async def discover(self) -> Hits:
        screen = self.screen
        for item in KANBAN_ACTIONS:
            if not _screen_allows_action(screen, item.action):
                continue
            yield DiscoveryHit(
                item.command,
                partial(_run_screen_action, screen, item.action),
                help=item.help,
            )
