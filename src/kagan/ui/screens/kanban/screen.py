"""Main Kanban board screen."""

from __future__ import annotations

import asyncio
import platform
from contextlib import suppress
from pathlib import Path
from typing import TYPE_CHECKING

from textual import getters, on
from textual.binding import Binding
from textual.containers import Container, Horizontal
from textual.css.query import NoMatches
from textual.widgets import Static

from kagan.constants import (
    COLUMN_ORDER,
    MIN_SCREEN_HEIGHT,
    MIN_SCREEN_WIDTH,
    NOTIFICATION_TITLE_MAX_LENGTH,
    STATUS_LABELS,
)
from kagan.core.models.enums import (
    CardIndicator,
    RejectionAction,
    ReviewResult,
    TaskStatus,
    TaskType,
)
from kagan.git_utils import has_git_repo, list_local_branches
from kagan.keybindings import KANBAN_BINDINGS
from kagan.services.workspaces import RepoWorkspaceInput
from kagan.ui.modals import (
    BaseBranchModal,
    ConfirmModal,
    DiffModal,
    ModalAction,
    RejectionInputModal,
    ReviewModal,
    TaskDetailsModal,
)
from kagan.ui.modals.description_editor import DescriptionEditorModal
from kagan.ui.screens.base import KaganScreen
from kagan.ui.screens.kanban import focus
from kagan.ui.screens.kanban.hints import build_kanban_hints
from kagan.ui.screens.planner import PlannerScreen
from kagan.ui.utils import copy_with_notification
from kagan.ui.widgets.card import TaskCard
from kagan.ui.widgets.column import KanbanColumn
from kagan.ui.widgets.header import KaganHeader
from kagan.ui.widgets.keybinding_hint import KanbanHintBar
from kagan.ui.widgets.offline_banner import OfflineBanner
from kagan.ui.widgets.peek_overlay import PeekOverlay
from kagan.ui.widgets.search_bar import SearchBar

if TYPE_CHECKING:
    from collections.abc import Sequence

    from textual import events
    from textual.app import ComposeResult
    from textual.timer import Timer

    from kagan.config import AgentConfig
    from kagan.core.models.entities import Task
    from kagan.mcp.global_config import GlobalMcpSpec

SIZE_WARNING_MESSAGE = (
    f"Terminal too small\n\n"
    f"Minimum size: {MIN_SCREEN_WIDTH}x{MIN_SCREEN_HEIGHT}\n"
    f"Please resize your terminal"
)
VALID_PAIR_LAUNCHERS = {"tmux", "vscode", "cursor"}


class KanbanScreen(KaganScreen):
    """Main Kanban board screen with 4 columns."""

    BINDINGS = KANBAN_BINDINGS

    header = getters.query_one(KaganHeader)

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self._tasks: list[Task] = []
        self._filtered_tasks: Sequence[Task] | None = None
        self._pending_delete_task: Task | None = None
        self._pending_merge_task: Task | None = None
        self._pending_close_task: Task | None = None
        self._pending_advance_task: Task | None = None
        self._pending_auto_move_task: Task | None = None
        self._pending_auto_move_status: TaskStatus | None = None
        self._editing_task_id: str | None = None
        self._refresh_timer: Timer | None = None
        self._task_hashes: dict[str, int] = {}
        self._agent_offline: bool = False
        self._merge_failed_tasks: set[str] = set()

    _TASK_REQUIRED_ACTIONS = frozenset(
        {
            "edit_task",
            "delete_task",
            "delete_task_direct",
            "view_details",
            "open_session",
            "move_forward",
            "move_backward",
            "duplicate_task",
            "merge",
            "merge_direct",
            "view_diff",
            "open_review",
            "start_agent",
            "stop_agent",
        }
    )

    _AGENT_REQUIRED_ACTIONS = frozenset(
        {
            "start_agent",
            "open_planner",
        }
    )

    def _validate_action(self, action: str) -> tuple[bool, str | None]:
        """Validate if an action can be performed (inlined from ActionValidator)."""
        if self._agent_offline and action in self._AGENT_REQUIRED_ACTIONS:
            return (False, "Agent unavailable (offline mode)")

        card = focus.get_focused_card(self)
        task = card.task_model if card else None
        scheduler = self.ctx.automation_service

        if not task:
            if action in self._TASK_REQUIRED_ACTIONS:
                return (False, "No task selected")
            return (True, None)

        status = task.status
        task_type = task.task_type

        if action == "edit_task":
            if status == TaskStatus.DONE:
                return (False, "Done tasks cannot be edited. Use [y] to duplicate.")
            return (True, None)

        if action in ("move_forward", "move_backward"):
            if status == TaskStatus.DONE:
                return (False, "Done tasks cannot be moved. Use [y] to duplicate.")
            return (True, None)

        if action in ("merge", "merge_direct", "view_diff", "open_review", "rebase"):
            if status != TaskStatus.REVIEW:
                return (False, f"Only available for REVIEW tasks (current: {status.value})")
            return (True, None)

        if action == "start_agent":
            if task_type != TaskType.AUTO:
                return (False, "Only available for AUTO tasks")
            return (True, None)

        if action == "stop_agent":
            if task_type != TaskType.AUTO:
                return (False, "Only available for AUTO tasks")
            if not scheduler.is_running(task.id):
                return (False, "No agent running for this task")
            return (True, None)

        return (True, None)

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool | None:
        is_valid, _ = self._validate_action(action)
        return True if is_valid else None

    def compose(self) -> ComposeResult:
        yield KaganHeader(task_count=0)
        yield SearchBar(id="search-bar")
        with Container(classes="board-container"):
            with Horizontal(classes="board"):
                for status in COLUMN_ORDER:
                    yield KanbanColumn(status=status, tasks=[])
        with Container(classes="size-warning"):
            yield Static(SIZE_WARNING_MESSAGE, classes="size-warning-text")
        yield Static("", id="review-queue-hint", classes="review-queue-hint")
        yield PeekOverlay(id="peek-overlay")
        yield KanbanHintBar(id="kanban-hint-bar")

    async def on_mount(self) -> None:
        self._check_screen_size()
        self._check_agent_health()
        await self.sync_header_context(self.header)
        await self._refresh_board()
        focus.focus_first_card(self)
        self.kagan_app.task_changed_signal.subscribe(self, self._on_task_changed)
        self._sync_agent_states()
        from kagan.ui.widgets.header import _get_git_branch

        if self.ctx.active_repo_id is None:
            self.header.update_branch("")
            return
        branch = await _get_git_branch(self.kagan_app.project_root)
        self.header.update_branch(branch)

    def _check_agent_health(self) -> None:
        """Check if agent is available and show banner if not."""
        if not self.ctx.agent_health.is_available():
            self._agent_offline = True
            message = self.ctx.agent_health.get_status_message()
            banner = OfflineBanner(message=message)
            self.mount(banner, before=self.query_one(".board-container"))
        else:
            self._agent_offline = False

    def on_unmount(self) -> None:
        """Clean up pending state on unmount."""
        self._pending_delete_task = None
        self._pending_merge_task = None
        self._pending_advance_task = None
        self._pending_auto_move_task = None
        self._pending_auto_move_status = None
        self._editing_task_id = None
        self._filtered_tasks = None
        self._merge_failed_tasks.clear()
        if self._refresh_timer:
            self._refresh_timer.stop()
            self._refresh_timer = None

    async def _on_task_changed(self, _task_id: str) -> None:
        self._schedule_refresh()

    def _sync_agent_states(self) -> None:
        """Sync agent active states and card indicators for all columns."""
        scheduler = self.ctx.automation_service
        running_tasks = scheduler.running_tasks
        indicators: dict[str, CardIndicator] = {}

        for tid in running_tasks:
            indicators[tid] = (
                CardIndicator.REVIEWING if scheduler.is_reviewing(tid) else CardIndicator.RUNNING
            )

        for task in self._tasks:
            if task.id in indicators:
                continue
            if task.id in self._merge_failed_tasks:
                indicators[task.id] = CardIndicator.FAILED
            elif task.status == TaskStatus.IN_PROGRESS and task.task_type == TaskType.AUTO:
                indicators[task.id] = CardIndicator.IDLE
            elif task.status == TaskStatus.DONE:
                indicators[task.id] = CardIndicator.PASSED

        for column in self.query(KanbanColumn):
            column.update_active_states(running_tasks, indicators)

    def _set_card_indicator(
        self,
        task_id: str,
        indicator: CardIndicator,
        *,
        is_active: bool | None = None,
    ) -> None:
        """Apply an immediate indicator update for a single visible card."""
        if is_active is None:
            is_active = indicator in {CardIndicator.RUNNING, CardIndicator.REVIEWING}
        for column in self.query(KanbanColumn):
            for card in column.get_cards():
                if card.task_model is None or card.task_model.id != task_id:
                    continue
                card.is_agent_active = is_active
                card.indicator = indicator
                return

    def on_descendant_focus(self, event: events.DescendantFocus) -> None:
        """Update UI immediately on focus change (hints first for instant feedback)."""
        self._update_keybinding_hints()
        self.refresh_bindings()

    def on_resize(self, event: events.Resize) -> None:
        self._check_screen_size()

    async def on_screen_resume(self) -> None:
        await self._refresh_board()
        self._sync_agent_states()

    async def reset_for_repo_change(self) -> None:
        """Reset board state when the active repo changes."""
        with suppress(NoMatches):
            search_bar = self.query_one("#search-bar", SearchBar)
            if search_bar.is_visible:
                search_bar.hide()
                search_bar.clear()
        self._filtered_tasks = None
        self._task_hashes.clear()
        self._tasks = []
        await self._refresh_and_sync()
        focus.focus_first_card(self)

    @on(OfflineBanner.Reconnect)
    def on_offline_banner_reconnect(self, event: OfflineBanner.Reconnect) -> None:
        """Handle reconnect from offline banner - refresh agent health check."""
        self.ctx.agent_health.refresh()
        if self.ctx.agent_health.is_available():
            self._agent_offline = False

            for banner in self.query(OfflineBanner):
                banner.remove()
            self.notify("Agent is now available", severity="information")
        else:
            self.notify("Agent still unavailable", severity="warning")

    def _check_screen_size(self) -> None:
        size = self.app.size
        if size.width < MIN_SCREEN_WIDTH or size.height < MIN_SCREEN_HEIGHT:
            self.add_class("too-small")
        else:
            self.remove_class("too-small")

    async def _refresh_board(self) -> None:
        """Refresh board with differential updates (only changed tasks)."""
        focused_task_id = None
        focused = self.app.focused
        if isinstance(focused, TaskCard) and focused.task_model:
            focused_task_id = focused.task_model.id

        new_tasks = await self.ctx.task_service.list_tasks(project_id=self.ctx.active_project_id)
        display_tasks = self._filtered_tasks if self._filtered_tasks is not None else new_tasks

        old_status_by_id = {task.id: task.status for task in self._tasks}

        new_hashes = {
            t.id: hash(
                (
                    t.status.value,
                    t.title,
                    t.description,
                    t.updated_at,
                )
            )
            for t in new_tasks
        }
        changed_ids = {tid for tid, h in new_hashes.items() if self._task_hashes.get(tid) != h}
        deleted_ids = set(self._task_hashes.keys()) - set(new_hashes.keys())

        if changed_ids or deleted_ids or self._task_hashes == {}:
            self._notify_status_changes(new_tasks, old_status_by_id, changed_ids)
            self._tasks = new_tasks
            self._task_hashes = new_hashes

            affected_statuses = set()
            for task in new_tasks:
                if task.id in changed_ids:
                    affected_statuses.add(task.status)
                    old_status = old_status_by_id.get(task.id)
                    if old_status is not None and old_status != task.status:
                        affected_statuses.add(old_status)
            for _tid in deleted_ids:
                affected_statuses = set(COLUMN_ORDER)
                break

            for status in affected_statuses:
                with suppress(NoMatches):
                    column = self.query_one(f"#column-{status.value.lower()}", KanbanColumn)
                    column.update_tasks([t for t in display_tasks if t.status == status])

            with suppress(NoMatches):
                self.header.update_count(len(self._tasks))
            self._update_review_queue_hint()
            self._update_keybinding_hints()
            self.refresh_bindings()
            if focused_task_id:
                self._restore_focus(focused_task_id)

        self._sync_agent_states()

    def _notify_status_changes(
        self,
        new_tasks: list[Task],
        old_status_by_id: dict[str, TaskStatus],
        changed_ids: set[str],
    ) -> None:
        """Show toasts for task status transitions detected during refresh."""
        transitions: list[tuple[Task, TaskStatus]] = []
        for task in new_tasks:
            if task.id not in changed_ids:
                continue
            old_status = old_status_by_id.get(task.id)
            if old_status is None or old_status == task.status:
                continue
            transitions.append((task, old_status))

        if not transitions:
            return

        max_toasts = 3
        for task, old_status in transitions[:max_toasts]:
            old_label = STATUS_LABELS.get(old_status, old_status.value.upper())
            new_label = STATUS_LABELS.get(task.status, task.status.value.upper())
            title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
            self.notify(
                f"#{task.short_id} {title}: {old_label} -> {new_label}",
                severity="information",
            )

        remaining = len(transitions) - max_toasts
        if remaining > 0:
            self.notify(
                f"{remaining} more task(s) changed status",
                severity="information",
            )

    def _restore_focus(self, task_id: str) -> None:
        for column in self.query(KanbanColumn):
            for card in column.get_cards():
                if card.task_model and card.task_model.id == task_id:
                    card.focus()
                    return

    async def _refresh_and_sync(self) -> None:
        await self._refresh_board()

    def _schedule_refresh(self) -> None:
        if self._refresh_timer:
            self._refresh_timer.stop()
        self._refresh_timer = self.set_timer(0.15, self._run_refresh)

    def _run_refresh(self) -> None:
        self._refresh_timer = None
        self.run_worker(self._refresh_and_sync())

    def _update_review_queue_hint(self) -> None:
        try:
            hint = self.query_one("#review-queue-hint", Static)
        except NoMatches:
            return
        review_count = sum(1 for task in self._tasks if task.status == TaskStatus.REVIEW)
        if review_count > 1:
            hint.update("Hint: multiple tasks are in REVIEW. Merging in order reduces conflicts.")
            hint.add_class("visible")
        else:
            hint.update("")
            hint.remove_class("visible")

    def _update_keybinding_hints(self) -> None:
        """Update hints based on focused card context."""
        try:
            hint_bar = self.query_one("#kanban-hint-bar", KanbanHintBar)
        except NoMatches:
            return

        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            hints = build_kanban_hints(None, None)
        else:
            hints = build_kanban_hints(card.task_model.status, card.task_model.task_type)

        hint_bar.show_kanban_hints(hints.navigation, hints.actions, hints.global_hints)

    def action_focus_left(self) -> None:
        focus.focus_horizontal(self, -1)

    def action_focus_right(self) -> None:
        focus.focus_horizontal(self, 1)

    def action_focus_up(self) -> None:
        focus.focus_vertical(self, -1)

    def action_focus_down(self) -> None:
        focus.focus_vertical(self, 1)

    def action_deselect(self) -> None:
        try:
            overlay = self.query_one("#peek-overlay", PeekOverlay)
            if overlay.has_class("visible"):
                overlay.hide()
                return
        except NoMatches:
            pass
        try:
            search_bar = self.query_one("#search-bar", SearchBar)
            if search_bar.is_visible:
                search_bar.hide()
                self._filtered_tasks = None
                self.run_worker(self._refresh_board())
                return
        except NoMatches:
            pass
        self.app.set_focus(None)

    def action_quit(self) -> None:
        self.app.exit()

    def action_interrupt(self) -> None:
        self.app.exit()

    async def action_toggle_peek(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            return
        try:
            overlay = self.query_one("#peek-overlay", PeekOverlay)
        except NoMatches:
            return
        if not overlay.toggle():
            return

        task = card.task_model
        scheduler = self.ctx.automation_service
        task_type = task.task_type

        if task_type == TaskType.AUTO:
            if scheduler.is_running(task.id):
                run_count = scheduler.get_run_count(task.id)
                run_label = f" (Run {run_count})" if run_count > 0 else ""
                status = f"🟢 Running{run_label}"
            else:
                status = "⚪ Idle"
        else:
            is_active = await self.ctx.session_service.session_exists(task.id)
            status = "🟢 Session Active" if is_active else "⚪ No Active Session"

        scratchpad = await self.ctx.task_service.get_scratchpad(task.id)
        content = scratchpad if scratchpad else "(No scratchpad)"

        overlay.update_content(task.short_id, task.title, status, content)
        x_pos = min(card.region.x + card.region.width + 2, self.size.width - 55)
        y_pos = max(1, card.region.y)
        overlay.show_at(x_pos, y_pos)

    def on_key(self, event: events.Key) -> None:
        feedback_actions = {
            "delete_task_direct",
            "merge_direct",
            "rebase",
            "edit_task",
            "view_details",
            "open_session",
            "start_agent",
            "stop_agent",
            "view_diff",
            "open_review",
        }
        key_action_map = {
            b.key: b.action
            for b in KANBAN_BINDINGS
            if isinstance(b, Binding) and b.action in feedback_actions
        }
        if event.key in key_action_map:
            _, reason = self._validate_action(key_action_map[event.key])
            if reason:
                self.notify(reason, severity="warning")

    def action_toggle_search(self) -> None:
        try:
            search_bar = self.query_one("#search-bar", SearchBar)
            if search_bar.is_visible:
                search_bar.hide()
                self._filtered_tasks = None
                self.run_worker(self._refresh_board())
            else:
                search_bar.show()
        except NoMatches:
            pass

    @on(SearchBar.QueryChanged)
    async def on_search_query_changed(self, event: SearchBar.QueryChanged) -> None:
        query = event.query.strip()
        if not query:
            self._filtered_tasks = None
        else:
            self._filtered_tasks = await self.ctx.task_service.search(query)
        await self._refresh_board()

    def action_new_task(self) -> None:
        self.app.push_screen(TaskDetailsModal(), callback=self._on_task_modal_result)

    def action_new_auto_task(self) -> None:
        self.app.push_screen(
            TaskDetailsModal(initial_type=TaskType.AUTO),
            callback=self._on_task_modal_result,
        )

    async def _on_task_modal_result(self, result: ModalAction | dict | None) -> None:
        if isinstance(result, dict) and self._editing_task_id is None:
            task = await self.ctx.task_service.create_task(
                result.get("title", ""),
                result.get("description", ""),
                created_by=None,
            )
            # Exclude title and description as they were already set during create_task
            update_fields = {k: v for k, v in result.items() if k not in ("title", "description")}
            if update_fields:
                await self.ctx.task_service.update_fields(task.id, **update_fields)
            await self._refresh_board()
            self.notify(f"Created task: {task.title}")
        elif isinstance(result, dict) and self._editing_task_id is not None:
            await self.ctx.task_service.update_fields(self._editing_task_id, **result)
            await self._refresh_board()
            self.notify("Task updated")
            self._editing_task_id = None
        elif result == ModalAction.DELETE:
            self.action_delete_task()

    def action_edit_task(self) -> None:
        card = focus.get_focused_card(self)
        if card and card.task_model:
            self._editing_task_id = card.task_model.id
            self.app.push_screen(
                TaskDetailsModal(
                    task=card.task_model,
                    start_editing=True,
                ),
                callback=self._on_task_modal_result,
            )

    def action_delete_task(self) -> None:
        card = focus.get_focused_card(self)
        if card and card.task_model:
            self._pending_delete_task = card.task_model
            self.app.push_screen(
                ConfirmModal(title="Delete Task?", message=f'"{card.task_model.title}"'),
                callback=self._on_delete_confirmed,
            )

    async def _on_delete_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_delete_task:
            task = self._pending_delete_task
            if self.ctx.merge_service:
                await self.ctx.merge_service.delete_task(task)
            await self._refresh_board()
            self.notify(f"Deleted task: {task.title}")
            focus.focus_first_card(self)
        self._pending_delete_task = None

    def action_delete_task_direct(self) -> None:
        card = focus.get_focused_card(self)
        if card and card.task_model:
            self._pending_delete_task = card.task_model
            self.app.push_screen(
                ConfirmModal(title="Delete Task?", message=f'"{card.task_model.title}"'),
                callback=self._on_delete_confirmed,
            )

    async def action_merge_direct(self) -> None:
        task = self._get_review_task(focus.get_focused_card(self))
        if not task:
            return
        if self.ctx.merge_service and await self.ctx.merge_service.has_no_changes(task):
            self._confirm_close_no_changes(task)
            return
        await self._execute_merge(
            task,
            success_msg=f"Merged: {task.title}",
            track_failures=True,
        )

    async def action_rebase(self) -> None:
        task = self._get_review_task(focus.get_focused_card(self))
        if not task:
            return
        base = task.base_branch or self.ctx.config.general.default_base_branch
        self.notify("Rebasing... (this may take a few seconds)", severity="information")
        success, message, conflict_files = await self.ctx.workspace_service.rebase_onto_base(
            task.id, base
        )
        if success:
            self.notify(f"Rebased: {task.title}", severity="information")
        elif conflict_files:
            await self._handle_rebase_conflict(task, base, conflict_files)
        else:
            self.notify(f"Rebase failed: {message}", severity="error")

    async def _handle_rebase_conflict(
        self, task: Task, base_branch: str, conflict_files: list[str]
    ) -> None:
        """Handle rebase conflict: abort, build instructions, restart agent."""
        from datetime import datetime

        from kagan.agents.conflict_instructions import build_conflict_resolution_instructions

        # Abort the in-progress rebase to leave the worktree clean
        await self.ctx.workspace_service.abort_rebase(task.id)

        # Build conflict resolution instructions
        workspaces = await self.ctx.workspace_service.list_workspaces(task_id=task.id)
        branch_name = workspaces[0].branch_name if workspaces else f"task-{task.short_id}"
        instructions = build_conflict_resolution_instructions(
            source_branch=branch_name,
            target_branch=base_branch,
            conflict_files=conflict_files,
        )

        # Append instructions to task description (timestamped)
        timestamp = datetime.now().strftime("%Y-%m-%d %H:%M")
        separator = f"\n\n---\n_Rebase conflict detected ({timestamp}):_\n\n"
        current_desc = task.description or ""
        new_desc = current_desc + separator + instructions
        await self.ctx.task_service.update_fields(task.id, description=new_desc)

        # Move task to IN_PROGRESS
        await self.ctx.task_service.move(task.id, TaskStatus.IN_PROGRESS)

        # Auto-restart agent for AUTO tasks
        if task.task_type == TaskType.AUTO:
            refreshed = await self.ctx.task_service.get_task(task.id)
            if refreshed:
                await self.ctx.automation_service.spawn_for_task(refreshed)

        # Track as merge-failed for indicator
        self._merge_failed_tasks.add(task.id)

        await self._refresh_and_sync()
        n_files = len(conflict_files)
        self.notify(
            f"Rebase conflict: {n_files} file(s). Task moved to IN_PROGRESS.",
            severity="warning",
        )

    async def _move_task(self, forward: bool) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            return
        task = card.task_model
        status = task.status
        task_type = task.task_type

        new_status = TaskStatus.next_status(status) if forward else TaskStatus.prev_status(status)
        if new_status:
            if status == TaskStatus.IN_PROGRESS and task_type == TaskType.AUTO:
                self._pending_auto_move_task = task
                self._pending_auto_move_status = new_status
                title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
                destination = new_status.value.upper()
                self.app.push_screen(
                    ConfirmModal(
                        title="Stop Agent and Move Task?",
                        message=(
                            f"Stop agent, keep worktree/logs, and move '{title}' to {destination}?"
                        ),
                    ),
                    callback=self._on_auto_move_confirmed,
                )
                return

            if status == TaskStatus.REVIEW and new_status == TaskStatus.DONE:
                if self.ctx.merge_service and await self.ctx.merge_service.has_no_changes(task):
                    self._confirm_close_no_changes(task)
                    return
                self._pending_merge_task = task
                title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
                self.app.push_screen(
                    ConfirmModal(
                        title="Complete Task?",
                        message=f"Merge '{title}' and move to DONE?",
                    ),
                    callback=self._on_merge_confirmed,
                )
                return

            if (
                status == TaskStatus.IN_PROGRESS
                and task_type == TaskType.PAIR
                and new_status == TaskStatus.REVIEW
            ):
                self._pending_advance_task = task
                title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
                self.app.push_screen(
                    ConfirmModal(title="Advance to Review?", message=f"Move '{title}' to REVIEW?"),
                    callback=self._on_advance_confirmed,
                )
                return

            if (
                task_type == TaskType.AUTO
                and status == TaskStatus.IN_PROGRESS
                and new_status != TaskStatus.REVIEW
            ):
                self._set_card_indicator(task.id, CardIndicator.IDLE, is_active=False)

            await self.ctx.task_service.move(task.id, new_status)
            await self._refresh_board()
            self.notify(f"Moved #{task.id} to {new_status.value}")
            focus.focus_column(self, new_status)
        else:
            self.notify(f"Already in {'final' if forward else 'first'} status", severity="warning")

    async def _on_merge_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_merge_task:
            task = self._pending_merge_task
            await self._execute_merge(
                task,
                success_msg=f"Merged and completed: {task.title}",
                track_failures=True,
            )
        self._pending_merge_task = None

    async def _on_close_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_close_task:
            task = self._pending_close_task
            success, message = (
                await self.ctx.merge_service.close_exploratory(task)
                if self.ctx.merge_service
                else (False, "")
            )
            if success:
                await self._refresh_board()
                self.notify(f"Closed (no changes): {task.title}")
            else:
                self.notify(message, severity="error")
        self._pending_close_task = None

    async def _on_advance_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_advance_task:
            task = self._pending_advance_task
            await self.ctx.task_service.update_fields(task.id, status=TaskStatus.REVIEW)
            await self._refresh_board()
            self.notify(f"Moved #{task.id} to REVIEW")
            focus.focus_column(self, TaskStatus.REVIEW)
        self._pending_advance_task = None

    async def _on_auto_move_confirmed(self, confirmed: bool | None) -> None:
        task = self._pending_auto_move_task
        new_status = self._pending_auto_move_status
        self._pending_auto_move_task = None
        self._pending_auto_move_status = None

        if not confirmed or task is None or new_status is None:
            return

        scheduler = self.ctx.automation_service
        if scheduler.is_running(task.id):
            await scheduler.stop_task(task.id)

        self._set_card_indicator(task.id, CardIndicator.IDLE, is_active=False)

        await self.ctx.task_service.move(task.id, new_status)
        await self._refresh_board()
        self.notify(f"Moved #{task.id} to {new_status.value} (agent stopped)")
        focus.focus_column(self, new_status)

    async def action_move_forward(self) -> None:
        await self._move_task(forward=True)

    async def action_move_backward(self) -> None:
        await self._move_task(forward=False)

    async def action_duplicate_task(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            self.notify("No task selected", severity="warning")
            return
        from kagan.ui.modals.duplicate_task import DuplicateTaskModal

        self.app.push_screen(
            DuplicateTaskModal(source_task=card.task_model),
            callback=self._on_duplicate_result,
        )

    async def _on_duplicate_result(self, result: dict | None) -> None:
        if result:
            task = await self.ctx.task_service.create_task(
                result.get("title", ""),
                result.get("description", ""),
                created_by=None,
            )
            # Exclude title and description as they were already set during create_task
            update_fields = {k: v for k, v in result.items() if k not in ("title", "description")}
            if update_fields:
                await self.ctx.task_service.update_fields(task.id, **update_fields)
            await self._refresh_board()
            self.notify(f"Created duplicate: #{task.short_id}")
            focus.focus_column(self, TaskStatus.BACKLOG)

    def action_copy_task_id(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            self.notify("No task selected", severity="warning")
            return
        copy_with_notification(self.app, f"#{card.task_model.short_id}", "Task ID")

    def action_view_details(self) -> None:
        card = focus.get_focused_card(self)
        if card and card.task_model:
            self._editing_task_id = card.task_model.id
            self.app.push_screen(
                TaskDetailsModal(
                    task=card.task_model,
                ),
                callback=self._on_task_modal_result,
            )

    def action_expand_description(self) -> None:
        """Expand description in full-screen editor (read-only from Kanban)."""
        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            self.notify("No task selected", severity="warning")
            return
        description = card.task_model.description or ""
        modal = DescriptionEditorModal(
            description=description, readonly=True, title="View Description"
        )
        self.app.push_screen(modal)

    def action_open_session(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            return
        self.run_worker(
            self._open_session_flow(card.task_model),
            group=f"open-session-{card.task_model.id}",
        )

    async def _provision_workspace_for_active_repo(self, task: Task) -> Path | None:
        active_repo_id = self.ctx.active_repo_id
        if active_repo_id is None:
            self.notify("Select a repository to start a session", severity="warning")
            return None

        repo_details = await self.ctx.project_service.get_project_repo_details(task.project_id)
        repo = next((item for item in repo_details if item["id"] == active_repo_id), None)
        if repo is None:
            self.notify("Active repository not part of this project", severity="error")
            return None

        repo_path = Path(repo["path"])
        if not await has_git_repo(repo_path):
            self.notify(f"Not a git repository: {repo_path}. Run git init first.", severity="error")
            return None

        self.notify("Creating workspace...", severity="information")
        try:
            await self.ctx.workspace_service.provision(
                task_id=task.id,
                repos=[
                    RepoWorkspaceInput(
                        repo_id=repo["id"],
                        repo_path=repo["path"],
                        target_branch=repo["default_branch"],
                    )
                ],
            )
        except Exception as exc:
            self.notify(f"Failed to create workspace: {exc}", severity="error")
            return None

        wt_path = await self.ctx.workspace_service.get_path(task.id)
        if wt_path is None:
            self.notify("Failed to provision workspace", severity="error")
            return None
        return wt_path

    async def _ensure_mcp_installed(self, agent_config: AgentConfig, spec: GlobalMcpSpec) -> bool:
        """Show MCP install modal and return True if install succeeded."""
        from kagan.ui.modals.mcp_install import McpInstallModal

        future: asyncio.Future[bool] = asyncio.get_event_loop().create_future()

        def on_result(result: bool | None) -> None:
            if not future.done():
                future.set_result(result is True)

        self.app.push_screen(
            McpInstallModal(agent_config=agent_config, spec=spec),
            callback=on_result,
        )
        return await future

    async def _handle_missing_agent(self, task: Task, agent_config: AgentConfig) -> str:
        """Show modal when task's agent is not installed. Returns action taken."""
        from kagan.builtin_agents import get_builtin_agent, list_available_agents
        from kagan.ui.modals.agent_choice import AgentChoiceModal, AgentChoiceResult

        builtin = get_builtin_agent(agent_config.short_name)
        available = list_available_agents()

        future: asyncio.Future[str] = asyncio.get_event_loop().create_future()

        def on_result(result: str | None) -> None:
            if not future.done():
                future.set_result(result or AgentChoiceResult.CANCELLED)

        self.app.push_screen(
            AgentChoiceModal(
                missing_agent=builtin,
                available_agents=available,
                task_title=task.title,
            ),
            callback=on_result,
        )

        return await future

    async def _update_task_agent(self, task: Task, agent_short_name: str) -> None:
        """Update task to use a different agent."""
        # Update task metadata or config
        # This depends on how tasks store their agent assignment
        # For now, we'll use a notification approach
        current_agent = task.get_agent_config(self.kagan_app.config).short_name
        self.notify(
            f"Using {agent_short_name} instead of {current_agent}",
            title="Agent Changed",
            timeout=3,
        )

    async def _ask_confirmation(self, title: str, message: str) -> bool:
        future: asyncio.Future[bool] = asyncio.get_event_loop().create_future()

        def on_result(result: bool | None) -> None:
            if not future.done():
                future.set_result(result is True)

        self.app.push_screen(ConfirmModal(title=title, message=message), callback=on_result)
        return await future

    async def _confirm_start_auto_task(self, task: Task) -> bool:
        title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
        return await self._ask_confirmation(
            "Start Agent?",
            f"Start agent for '{title}' and open output stream?",
        )

    async def _confirm_attach_pair_session(self, task: Task) -> bool:
        title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
        return await self._ask_confirmation(
            "Attach Session?",
            f"Attach to session for '{title}'?",
        )

    async def _open_auto_output_for_task(
        self,
        task: Task,
        *,
        wait_for_running: bool = False,
    ) -> None:
        scheduler = self.ctx.automation_service

        if wait_for_running:
            for _ in range(20):
                if scheduler.is_running(task.id):
                    break
                await asyncio.sleep(0.1)

        if not scheduler.is_running(task.id):
            latest = await self.ctx.execution_service.get_latest_execution_for_task(task.id)
            if latest is None:
                self.notify("No agent logs available for this task", severity="warning")
                return

        await self._open_review_for_task(
            task,
            read_only=True,
            initial_tab="review-agent-output",
            include_running_output=True,
        )

    async def _open_session_flow(self, task: Task) -> None:
        refreshed = await self.ctx.task_service.get_task(task.id)
        if refreshed is not None:
            task = refreshed

        if task.status in (TaskStatus.REVIEW, TaskStatus.DONE):
            await self._open_review_for_task(task, read_only=task.status == TaskStatus.DONE)
            return

        if task.task_type == TaskType.AUTO:
            if task.status == TaskStatus.BACKLOG:
                if not await self._confirm_start_auto_task(task):
                    return
                await self._start_agent_flow(task)
                refreshed_after_start = await self.ctx.task_service.get_task(task.id)
                await self._open_auto_output_for_task(
                    refreshed_after_start or task,
                    wait_for_running=True,
                )
            elif task.status == TaskStatus.IN_PROGRESS:
                await self._open_auto_output_for_task(task)
            return

        # ── Agent Availability Gate ───────────────────────────────
        agent_config = task.get_agent_config(self.kagan_app.config)
        from kagan.agents.installer import check_agent_installed

        if not check_agent_installed(agent_config.short_name):
            from kagan.ui.modals.agent_choice import AgentChoiceResult

            result = await self._handle_missing_agent(task, agent_config)
            if result == AgentChoiceResult.CANCELLED:
                return
            if result == AgentChoiceResult.INSTALLED:
                pass  # Continue with same agent
            elif fallback_agent := AgentChoiceResult.parse_fallback(result):
                # User selected fallback agent - update task
                await self._update_task_agent(task, fallback_agent)
                # Refresh agent_config
                refreshed = await self.ctx.task_service.get_task(task.id)
                if refreshed:
                    task = refreshed
                    agent_config = task.get_agent_config(self.kagan_app.config)
        # ── End Agent Gate ───────────────────────────────

        # ── MCP Gate ───────────────────────────────────
        from kagan.mcp.global_config import get_global_mcp_spec, is_global_mcp_configured

        if not is_global_mcp_configured(agent_config.short_name):
            spec = get_global_mcp_spec(agent_config.short_name)
            if spec:
                installed = await self._ensure_mcp_installed(agent_config, spec)
                if not installed:
                    return
        # ── End MCP Gate ───────────────────────────────

        wt_path = await self.ctx.workspace_service.get_path(task.id)
        if wt_path is None:
            wt_path = await self._provision_workspace_for_active_repo(task)
            if wt_path is None:
                return

        if not await self._ensure_pair_terminal_backend_ready(task):
            return

        terminal_backend = self._resolve_pair_terminal_backend(task)

        if not await self.ctx.session_service.session_exists(task.id):
            self.notify("Creating session...", severity="information")
            await self.ctx.session_service.create_session(task, wt_path)

        if task.status == TaskStatus.IN_PROGRESS and not await self._confirm_attach_pair_session(
            task
        ):
            return

        if not self.kagan_app.config.ui.skip_pair_instructions:
            from kagan.ui.modals.tmux_gateway import PairInstructionsModal

            def on_gateway_result(result: str | None) -> None:
                if result is None:
                    return
                if result == "skip_future":
                    self.kagan_app.config.ui.skip_pair_instructions = True
                    cb_result = self._save_pair_instructions_preference(skip=True)
                    if asyncio.iscoroutine(cb_result):
                        asyncio.create_task(cb_result)

                self.app.call_later(self._do_open_pair_session, task, wt_path, terminal_backend)

            self.app.push_screen(
                PairInstructionsModal(
                    task.id,
                    task.title,
                    terminal_backend,
                    self._startup_prompt_path_hint(wt_path),
                ),
                on_gateway_result,
            )
            return

        await self._do_open_pair_session(task, wt_path, terminal_backend)

    def _resolve_pair_terminal_backend(self, task: Task) -> str:
        task_backend = getattr(task, "terminal_backend", None)
        if isinstance(task_backend, str):
            normalized = task_backend.strip().lower()
            if normalized in VALID_PAIR_LAUNCHERS:
                return normalized

        configured = getattr(
            self.kagan_app.config.general,
            "default_pair_terminal_backend",
            "tmux",
        )
        if isinstance(configured, str):
            normalized = configured.strip().lower()
            if normalized in VALID_PAIR_LAUNCHERS:
                return normalized

        return "tmux"

    async def _ensure_pair_terminal_backend_ready(self, task: Task) -> bool:
        from kagan.terminals.installer import check_terminal_installed, first_available_pair_backend
        from kagan.ui.modals.terminal_install import TerminalInstallModal

        backend = self._resolve_pair_terminal_backend(task)
        is_windows = platform.system() == "Windows"

        if backend in {"vscode", "cursor"}:
            if check_terminal_installed(backend):
                return True
            self.notify(
                f"{backend} is not installed. Install it and retry.",
                severity="warning",
            )
            return False

        if backend == "tmux":
            if check_terminal_installed("tmux"):
                return True
            if is_windows:
                fallback = first_available_pair_backend(windows=True)
                if fallback is not None:
                    await self.ctx.task_service.update_fields(task.id, terminal_backend=fallback)
                    self.notify(
                        f"tmux not found on Windows. Using {fallback} for this task.",
                        severity="information",
                    )
                    return True
                self.notify(
                    "PAIR cancelled: install VS Code or Cursor to continue on Windows.",
                    severity="warning",
                )
                return False

            installed_tmux = await self.app.push_screen(TerminalInstallModal("tmux"))
            if installed_tmux and check_terminal_installed("tmux"):
                return True

            fallback = first_available_pair_backend(windows=False)
            if fallback is not None:
                await self.ctx.task_service.update_fields(task.id, terminal_backend=fallback)
                self.notify(
                    "tmux not installed. Using fallback launcher "
                    f"{fallback}. VS Code: https://code.visualstudio.com/download "
                    "Cursor: https://cursor.com/downloads",
                    severity="information",
                )
                return True
            self.notify(
                "PAIR cancelled: install tmux (recommended), or install VS Code/Cursor "
                "for external development.",
                severity="warning",
            )
            return False

        self.notify(f"Unsupported PAIR launcher: {backend}", severity="warning")
        return False

    @staticmethod
    def _startup_prompt_path_hint(workspace_path: Path) -> Path:
        return workspace_path / ".kagan" / "start_prompt.md"

    async def _do_open_pair_session(
        self,
        task: Task,
        workspace_path: Path | None = None,
        terminal_backend: str | None = None,
    ) -> None:
        """Open the pair session after modal confirmation."""
        try:
            if task.status == TaskStatus.BACKLOG:
                await self.ctx.task_service.update_fields(task.id, status=TaskStatus.IN_PROGRESS)
                await self._refresh_board()

            with self.app.suspend():
                attached = await self.ctx.session_service.attach_session(task.id)

            backend = terminal_backend or self._resolve_pair_terminal_backend(task)
            if not attached:
                self.notify("Failed to open PAIR session", severity="warning")
                return

            if backend != "tmux":
                prompt_path = (
                    self._startup_prompt_path_hint(workspace_path)
                    if workspace_path is not None
                    else Path(".kagan/start_prompt.md")
                )
                self.notify(
                    f"Workspace opened externally. Use startup prompt: {prompt_path}",
                    severity="information",
                )
                return

            session_still_exists = await self.ctx.session_service.session_exists(task.id)
            if session_still_exists:
                return

            from kagan.ui.modals.confirm import ConfirmModal

            def on_confirm(result: bool | None) -> None:
                if result:

                    async def move_to_review() -> None:
                        await self.ctx.task_service.update_fields(task.id, status=TaskStatus.REVIEW)
                        await self._refresh_board()

                    self.app.call_later(move_to_review)

            self.app.push_screen(
                ConfirmModal("Session Complete", "Move task to REVIEW?"),
                on_confirm,
            )

        except Exception as e:
            from kagan.tmux import TmuxError

            if isinstance(e, TmuxError):
                self.notify(f"Tmux error: {e}", severity="error")

    def action_start_agent(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            return
        self.run_worker(
            self._start_agent_flow(card.task_model),
            group=f"start-agent-{card.task_model.id}",
        )

    async def _start_agent_flow(self, task: Task) -> None:
        if task.task_type == TaskType.PAIR:
            return

        if self.ctx.automation_service.is_running(task.id):
            self.notify("Agent already running for this task (press Enter to open output)")
            return

        wt_path = await self.ctx.workspace_service.get_path(task.id)
        if wt_path is None:
            wt_path = await self._provision_workspace_for_active_repo(task)
            if wt_path is None:
                return

        if task.status == TaskStatus.BACKLOG:
            await self.ctx.task_service.move(task.id, TaskStatus.IN_PROGRESS)
            refreshed = await self.ctx.task_service.get_task(task.id)
            if refreshed:
                task = refreshed
            await self._refresh_board()

        self.notify("Starting agent...", severity="information")

        result = self.ctx.automation_service.spawn_for_task(task)

        if hasattr(result, "__await__"):
            spawned = await result
        else:
            spawned = result

        if spawned:
            self._set_card_indicator(task.id, CardIndicator.RUNNING, is_active=True)
            self.notify(f"Agent started: {task.id[:8]}", severity="information")
        else:
            self.notify("Failed to start agent (at capacity?)", severity="warning")

    async def action_stop_agent(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            return
        task = card.task_model

        if not self.ctx.automation_service.is_running(task.id):
            self.notify("No agent running for this task", severity="warning")
            return

        self.notify("Stopping agent...", severity="information")

        result = self.ctx.automation_service.stop_task(task.id)

        if hasattr(result, "__await__"):
            await result

        self._set_card_indicator(task.id, CardIndicator.IDLE, is_active=False)
        self.notify(f"Agent stopped: {task.id[:8]}", severity="information")

    def action_open_planner(self) -> None:
        self.app.push_screen(PlannerScreen(agent_factory=self.kagan_app._agent_factory))

    def action_switch_global_agent(self) -> None:
        """Open global agent picker."""
        from kagan.ui.modals import GlobalAgentPickerModal

        current_agent = self.kagan_app.config.general.default_worker_agent
        self.app.push_screen(
            GlobalAgentPickerModal(current_agent=current_agent),
            callback=self._on_global_agent_selected,
        )

    def _on_global_agent_selected(self, selected: str | None) -> None:
        """Apply selected global agent after modal dismiss."""
        if not selected:
            return
        self.run_worker(
            self._apply_global_agent_selection(selected),
            exclusive=True,
            exit_on_error=False,
        )

    async def _apply_global_agent_selection(self, selected: str) -> None:
        """Persist selected global agent and refresh header chip."""
        from kagan.builtin_agents import get_builtin_agent

        config = self.kagan_app.config
        current_agent = config.general.default_worker_agent
        if not selected or selected == current_agent:
            return

        config.general.default_worker_agent = selected
        if config.get_agent(selected) is None:
            if builtin := get_builtin_agent(selected):
                config.agents[selected] = builtin.config.model_copy(deep=True)

        await config.save(self.kagan_app.config_path)
        self.header.update_agent_from_config(config)
        self.notify(f"Global agent set to: {selected}", severity="information")

    async def action_open_settings(self) -> None:
        from kagan.ui.modals import SettingsModal

        config = self.kagan_app.config
        config_path = self.kagan_app.config_path
        result = await self.app.push_screen(SettingsModal(config, config_path))
        if result:
            self.kagan_app.config = self.kagan_app.config.load(config_path)
            self.header.update_agent_from_config(self.kagan_app.config)
            self.notify("Settings saved")

    def _get_review_task(self, card: TaskCard | None) -> Task | None:
        """Get task from card if it's in REVIEW status."""
        if not card or not card.task_model:
            return None
        if card.task_model.status != TaskStatus.REVIEW:
            self.notify("Task is not in REVIEW", severity="warning")
            return None
        return card.task_model

    async def action_merge(self) -> None:
        task = self._get_review_task(focus.get_focused_card(self))
        if not task:
            return
        if self.ctx.merge_service and await self.ctx.merge_service.has_no_changes(task):
            self._confirm_close_no_changes(task)
            return
        await self._execute_merge(task, success_msg=f"Merged and completed: {task.title}")

    async def action_view_diff(self) -> None:
        task = self._get_review_task(focus.get_focused_card(self))
        if not task:
            return
        title = f"Diff: {task.short_id} {task.title[:NOTIFICATION_TITLE_MAX_LENGTH]}"
        workspace_service = self.ctx.workspace_service
        workspaces = await workspace_service.list_workspaces(task_id=task.id)

        if not workspaces or self.ctx.diff_service is None:
            base = self.kagan_app.config.general.default_base_branch
            diff_text = await workspace_service.get_diff(task.id, base_branch=base)  # type: ignore[misc]
            await self.app.push_screen(
                DiffModal(title=title, diff_text=diff_text, task=task),
                callback=lambda result: self._on_diff_result(task, result),
            )
            return

        diffs = await self.ctx.diff_service.get_all_diffs(workspaces[0].id)
        await self.app.push_screen(
            DiffModal(title=title, diffs=diffs, task=task),
            callback=lambda result: self._on_diff_result(task, result),
        )

    async def _on_diff_result(self, task: Task, result: str | None) -> None:
        if result == ReviewResult.APPROVE:
            if self.ctx.merge_service and await self.ctx.merge_service.has_no_changes(task):
                self._confirm_close_no_changes(task)
                return
            await self._execute_merge(task, success_msg=f"Merged: {task.title}")
        elif result == ReviewResult.REJECT:
            await self._handle_reject_with_feedback(task)

    async def action_open_review(self) -> None:
        task = self._get_review_task(focus.get_focused_card(self))
        if not task:
            return
        await self._open_review_for_task(task)

    async def _open_review_for_task(
        self,
        task: Task,
        *,
        read_only: bool = False,
        initial_tab: str | None = None,
        include_running_output: bool = False,
    ) -> None:
        agent_config = task.get_agent_config(self.kagan_app.config)
        task_id = task.id
        scheduler = self.ctx.automation_service
        is_auto = task.task_type == TaskType.AUTO

        execution_id = scheduler.get_execution_id(task.id) if is_auto else None
        run_count = scheduler.get_run_count(task.id) if is_auto else 0
        is_running = scheduler.is_running(task.id) if is_auto else False
        is_reviewing = scheduler.is_reviewing(task.id) if is_auto else False
        review_agent = scheduler.get_review_agent(task.id) if is_auto else None
        running_agent = (
            scheduler.get_running_agent(task.id) if is_auto and include_running_output else None
        )

        if execution_id is None:
            latest = await self.ctx.execution_service.get_latest_execution_for_task(task.id)
            if latest is not None:
                execution_id = latest.id
                if run_count == 0:
                    run_count = await self.ctx.execution_service.count_executions_for_task(task.id)

        async def _handle_review(result: str | None) -> None:
            await self._on_review_result(task_id, result)

        await self.app.push_screen(
            ReviewModal(
                task=task,
                worktree_manager=self.ctx.workspace_service,
                agent_config=agent_config,
                base_branch=self.kagan_app.config.general.default_base_branch,
                agent_factory=self.kagan_app._agent_factory,
                execution_service=self.ctx.execution_service,
                execution_id=execution_id,
                run_count=run_count,
                running_agent=running_agent,
                review_agent=review_agent,
                is_reviewing=is_reviewing,
                is_running=is_running,
                read_only=read_only,
                initial_tab=initial_tab
                or (
                    "review-ai"
                    if is_auto and task.status == TaskStatus.REVIEW
                    else "review-summary"
                ),
            ),
            callback=_handle_review,
        )

    async def _on_review_result(self, task_id: str, result: str | None) -> None:
        task = await self.ctx.task_service.get_task(task_id)
        if not task:
            return

        if result == "rebase_conflict":
            base = task.base_branch or self.ctx.config.general.default_base_branch
            # Re-trigger rebase to get conflict file list
            _success, _msg, conflict_files = await self.ctx.workspace_service.rebase_onto_base(
                task.id, base
            )
            await self._handle_rebase_conflict(task, base, conflict_files)
            return

        if task.status != TaskStatus.REVIEW:
            return
        if result == ReviewResult.APPROVE:
            if self.ctx.merge_service and await self.ctx.merge_service.has_no_changes(task):
                self._confirm_close_no_changes(task)
                return
            await self._execute_merge(
                task,
                success_msg=f"Merged and completed: {task.title}",
                track_failures=True,
            )
        elif result == ReviewResult.EXPLORATORY:
            self._confirm_close_no_changes(task)
        elif result == ReviewResult.REJECT:
            await self._handle_reject_with_feedback(task)

    async def _execute_merge(
        self,
        task: Task,
        *,
        success_msg: str,
        track_failures: bool = False,
    ) -> bool:
        """Run merge_task and handle success/failure uniformly.

        Returns True if merge succeeded.
        """
        self.notify("Merging... (this may take a few seconds)", severity="information")
        success, message = (
            await self.ctx.merge_service.merge_task(task) if self.ctx.merge_service else (False, "")
        )
        if success:
            if track_failures:
                self._merge_failed_tasks.discard(task.id)
            await self._refresh_board()
            self.notify(success_msg, severity="information")
        else:
            if track_failures:
                self._merge_failed_tasks.add(task.id)
                self._sync_agent_states()
            self.notify(KanbanScreen._format_merge_failure(task, message), severity="error")
        return success

    @staticmethod
    def _format_merge_failure(task: Task, message: str) -> str:
        task_type = task.task_type
        if task_type == TaskType.AUTO:
            return f"Merge failed (AUTO): {message}"
        return f"Merge failed (PAIR): {message}"

    def _confirm_close_no_changes(self, task: Task) -> None:
        self._pending_close_task = task
        title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
        self.app.push_screen(
            ConfirmModal(
                title="No Changes Detected",
                message=f"Mark '{title}' as DONE and archive the workspace?",
            ),
            callback=self._on_close_confirmed,
        )

    async def _handle_reject_with_feedback(self, task: Task) -> None:
        await self.app.push_screen(
            RejectionInputModal(task.title),
            callback=lambda result: self._apply_rejection_result(task, result),
        )

    async def _apply_rejection_result(self, task: Task, result: tuple[str, str] | None) -> None:
        if self.ctx.merge_service is None:
            return
        if result is None:
            await self.ctx.merge_service.apply_rejection_feedback(
                task, None, RejectionAction.BACKLOG
            )
            action = RejectionAction.BACKLOG
        else:
            feedback, action = result
            await self.ctx.merge_service.apply_rejection_feedback(task, feedback, action)
        await self._refresh_board()
        if action == RejectionAction.BACKLOG:
            self.notify(f"Moved to BACKLOG: {task.title}")
        else:
            self.notify(f"Returned to IN_PROGRESS: {task.title}")

    async def _save_pair_instructions_preference(self, skip: bool = True) -> None:
        """Save pair instructions preference to config."""
        try:
            await self.kagan_app.config.update_ui_preferences(
                self.kagan_app.config_path,
                skip_pair_instructions=skip,
            )
        except Exception as e:
            self.notify(f"Failed to save preference: {e}", severity="error")

    async def action_set_task_branch(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            self.notify("No task focused", severity="warning")
            return

        task = card.task_model

        branches = await list_local_branches(self.kagan_app.project_root)

        def on_dismiss(branch: str | None) -> None:
            if branch is not None:
                asyncio.create_task(self._update_task_branch(task, branch))

        modal = BaseBranchModal(
            branches=branches,
            current_value=task.base_branch or "",
            title="Set Task Branch",
            description=f"Set base branch for: {task.title[:40]}",
        )
        self.app.push_screen(modal, on_dismiss)

    async def _update_task_branch(self, task: Task, branch: str) -> None:
        await self.ctx.task_service.update_fields(task.id, base_branch=branch or None)
        await self._refresh_board()
        self.notify(f"Branch set to: {branch or '(default)'}")

    async def action_set_default_branch(self) -> None:
        config = self.kagan_app.config

        branches = await list_local_branches(self.kagan_app.project_root)

        def on_dismiss(branch: str | None) -> None:
            if branch is not None:
                config.general.default_base_branch = branch or "main"
                self.notify(f"Default branch set to: {branch or 'main'}")

        modal = BaseBranchModal(
            branches=branches,
            current_value=config.general.default_base_branch,
            title="Set Default Branch",
            description="Set global default branch for new workspaces:",
        )
        self.app.push_screen(modal, on_dismiss)

    def on_task_card_selected(self, message: TaskCard.Selected) -> None:
        self.action_view_details()
