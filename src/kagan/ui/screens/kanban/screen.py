"""Main Kanban board screen."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from textual import getters, on
from textual.binding import Binding
from textual.containers import Container, Horizontal
from textual.css.query import NoMatches
from textual.widgets import Footer, Static

from kagan.constants import (
    COLUMN_ORDER,
    MIN_SCREEN_HEIGHT,
    MIN_SCREEN_WIDTH,
    NOTIFICATION_TITLE_MAX_LENGTH,
)
from kagan.core.models.enums import MergeReadiness, TaskStatus, TaskType
from kagan.keybindings import (
    KANBAN_BINDINGS,
    KANBAN_LEADER_BINDINGS,
    generate_leader_hint,
)
from kagan.ui.modals import (
    AgentOutputModal,
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
from kagan.ui.screens.kanban.hints import build_keybinding_hints
from kagan.ui.screens.planner import PlannerScreen
from kagan.ui.utils import copy_with_notification
from kagan.ui.widgets.card import TaskCard  # noqa: TC001 - needed at runtime for message handler
from kagan.ui.widgets.column import KanbanColumn
from kagan.ui.widgets.header import KaganHeader
from kagan.ui.widgets.keybinding_hint import KeybindingHint
from kagan.ui.widgets.peek_overlay import PeekOverlay
from kagan.ui.widgets.search_bar import SearchBar

if TYPE_CHECKING:
    from collections.abc import Sequence

    from textual import events
    from textual.app import ComposeResult
    from textual.timer import Timer

    from kagan.adapters.db.schema import AgentTurn
    from kagan.core.models.entities import Task

# Leader key timeout in seconds
LEADER_TIMEOUT = 2.0

SIZE_WARNING_MESSAGE = (
    f"Terminal too small\n\n"
    f"Minimum size: {MIN_SCREEN_WIDTH}x{MIN_SCREEN_HEIGHT}\n"
    f"Please resize your terminal"
)


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
        self._leader_active: bool = False
        self._leader_timer: Timer | None = None
        self._merge_readiness: dict[str, str] = {}
        self._refresh_timer: Timer | None = None
        self._task_hashes: dict[str, int] = {}  # task_id -> hash for change detection
        # Lifecycle handled via services in AppContext

    # Actions requiring a task to be selected
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
            "watch_agent",
            "start_agent",
            "stop_agent",
        }
    )

    def _validate_action(self, action: str) -> tuple[bool, str | None]:
        """Validate if an action can be performed (inlined from ActionValidator)."""
        card = focus.get_focused_card(self)
        task = card.task_model if card else None
        scheduler = self.ctx.automation_service

        # No task - check task-requiring actions
        if not task:
            if action in self._TASK_REQUIRED_ACTIONS:
                return (False, "No task selected")
            return (True, None)

        status = task.status
        task_type = task.task_type

        # Edit validation
        if action == "edit_task":
            if status == TaskStatus.DONE:
                return (False, "Done tasks cannot be edited. Use [y] to duplicate.")
            return (True, None)

        # Move validation
        if action in ("move_forward", "move_backward"):
            if status == TaskStatus.DONE:
                return (False, "Done tasks cannot be moved. Use [y] to duplicate.")
            return (True, None)

        # Review validation
        if action in ("merge", "merge_direct", "view_diff", "open_review"):
            if status != TaskStatus.REVIEW:
                return (False, f"Only available for REVIEW tasks (current: {status.value})")
            return (True, None)

        # Watch agent validation
        if action == "watch_agent":
            if task_type != TaskType.AUTO:
                return (False, "Only available for AUTO tasks")
            is_running = scheduler.is_running(task.id)
            if is_running or status == TaskStatus.IN_PROGRESS:
                return (True, None)
            return (False, "No agent running for this task")

        # Start agent validation
        if action == "start_agent":
            if task_type != TaskType.AUTO:
                return (False, "Only available for AUTO tasks")
            return (True, None)

        # Stop agent validation
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
        yield Static(generate_leader_hint(KANBAN_LEADER_BINDINGS), classes="leader-hint")
        yield PeekOverlay(id="peek-overlay")
        yield KeybindingHint(id="keybinding-hint", classes="keybinding-hint")
        yield Footer()

    async def on_mount(self) -> None:
        self._check_screen_size()
        await self._refresh_board()
        focus.focus_first_card(self)
        self.kagan_app.task_changed_signal.subscribe(self, self._on_task_changed)
        self.kagan_app.iteration_changed_signal.subscribe(self, self._on_iteration_changed)
        self._sync_iterations()
        self._sync_agent_states()
        from kagan.ui.widgets.header import _get_git_branch

        branch = await _get_git_branch(self.kagan_app.project_root)
        self.header.update_branch(branch)

    def on_unmount(self) -> None:
        """Clean up pending state on unmount."""
        self._pending_delete_task = None
        self._pending_merge_task = None
        self._pending_advance_task = None
        self._pending_auto_move_task = None
        self._pending_auto_move_status = None
        self._editing_task_id = None
        self._filtered_tasks = None
        if self._refresh_timer:
            self._refresh_timer.stop()
            self._refresh_timer = None

    async def _on_task_changed(self, _task_id: str) -> None:
        self._schedule_refresh()

    def _on_iteration_changed(self, data: tuple[str, int]) -> None:
        task_id, iteration = data
        try:
            column = self.query_one("#column-in_progress", KanbanColumn)
        except NoMatches:
            return
        max_iter = self.kagan_app.config.general.max_iterations
        if iteration > 0:
            column.update_iterations({task_id: f"Iter {iteration}/{max_iter}"})
            for card in column.get_cards():
                if card.task_model and card.task_model.id == task_id:
                    card.is_agent_active = True
        else:
            column.update_iterations({task_id: ""})
            for card in column.get_cards():
                if card.task_model and card.task_model.id == task_id:
                    card.is_agent_active = False

    def _sync_iterations(self) -> None:
        scheduler = self.ctx.automation_service
        try:
            column = self.query_one("#column-in_progress", KanbanColumn)
        except NoMatches:
            return
        max_iter = self.kagan_app.config.general.max_iterations
        iterations = {}
        for card in column.get_cards():
            if card.task_model:
                count = scheduler.get_iteration_count(card.task_model.id)
                if count > 0:
                    iterations[card.task_model.id] = f"Iter {count}/{max_iter}"
        if iterations:
            column.update_iterations(iterations)

    def _sync_agent_states(self) -> None:
        """Sync agent active states for all columns.

        Updates is_agent_active for all cards based on automation's running tasks.
        This ensures cards show correct running state even during status transitions.
        """
        scheduler = self.ctx.automation_service
        running_tasks = scheduler.running_tasks
        for column in self.query(KanbanColumn):
            column.update_active_states(running_tasks)

    def on_descendant_focus(self, event: events.DescendantFocus) -> None:
        """Update UI immediately on focus change (hints first for instant feedback)."""
        self._update_keybinding_hints()
        self.refresh_bindings()

    def on_resize(self, event: events.Resize) -> None:
        self._check_screen_size()

    async def on_screen_resume(self) -> None:
        await self._refresh_board()
        self._sync_iterations()
        self._sync_agent_states()

    def _check_screen_size(self) -> None:
        size = self.app.size
        if size.width < MIN_SCREEN_WIDTH or size.height < MIN_SCREEN_HEIGHT:
            self.add_class("too-small")
        else:
            self.remove_class("too-small")

    async def _refresh_board(self) -> None:
        """Refresh board with differential updates (only changed tasks)."""
        new_tasks = await self.ctx.task_service.list_tasks()
        display_tasks = self._filtered_tasks if self._filtered_tasks is not None else new_tasks

        old_status_by_id = {task.id: task.status for task in self._tasks}

        # Compute changed tasks
        new_hashes = {
            t.id: hash(
                (
                    t.status.value,
                    t.title,
                    t.session_active,
                    t.total_iterations,
                    t.last_error,
                )
            )
            for t in new_tasks
        }
        changed_ids = {tid for tid, h in new_hashes.items() if self._task_hashes.get(tid) != h}
        deleted_ids = set(self._task_hashes.keys()) - set(new_hashes.keys())

        # Only update if changes detected
        if changed_ids or deleted_ids or self._task_hashes == {}:
            self._tasks = new_tasks
            self._task_hashes = new_hashes
            self._update_merge_readiness_cache(new_tasks)

            # Determine which columns need updates
            affected_statuses = set()
            for task in new_tasks:
                if task.id in changed_ids:
                    affected_statuses.add(task.status)
                    old_status = old_status_by_id.get(task.id)
                    if old_status is not None and old_status != task.status:
                        affected_statuses.add(old_status)
            for _tid in deleted_ids:
                # Need full refresh on deletion to be safe
                affected_statuses = set(COLUMN_ORDER)
                break

            # Update only affected columns
            for status in affected_statuses:
                column = self.query_one(f"#column-{status.value.lower()}", KanbanColumn)
                column.update_tasks([t for t in display_tasks if t.status == status])

            self._sync_merge_readiness()
            self.header.update_count(len(self._tasks))
            active_sessions = sum(1 for task in self._tasks if task.session_active)
            self.header.update_sessions(active_sessions)
            self._update_review_queue_hint()
            self._update_keybinding_hints()
            self.refresh_bindings()

    async def _refresh_and_sync(self) -> None:
        await self._refresh_board()
        self._sync_iterations()
        self._sync_agent_states()

    def _schedule_refresh(self) -> None:
        if self._refresh_timer:
            self._refresh_timer.stop()
        self._refresh_timer = self.set_timer(0.15, self._run_refresh)

    def _run_refresh(self) -> None:
        self._refresh_timer = None
        self.run_worker(self._refresh_and_sync())

    def _update_merge_readiness_cache(self, tasks: list[Task]) -> None:
        for task in tasks:
            if task.status != TaskStatus.REVIEW:
                self._merge_readiness.pop(task.id, None)
                continue
            readiness_value = getattr(task, "merge_readiness", "risk")
            readiness = (
                readiness_value.value if hasattr(readiness_value, "value") else str(readiness_value)
            )
            if task.merge_failed:
                readiness = "blocked"
            self._merge_readiness[task.id] = readiness or "risk"

    def _sync_merge_readiness(self) -> None:
        for status in COLUMN_ORDER:
            try:
                column = self.query_one(f"#column-{status.value.lower()}", KanbanColumn)
            except NoMatches:
                continue
            column.update_merge_readiness(self._merge_readiness)

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
            hint_widget = self.query_one("#keybinding-hint", KeybindingHint)
        except NoMatches:
            return

        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            hints = build_keybinding_hints(None, None)
        else:
            hints = build_keybinding_hints(card.task_model.status, card.task_model.task_type)

        hint_widget.show_hints(hints)

    # =========================================================================
    # Navigation
    # =========================================================================

    def action_focus_left(self) -> None:
        focus.focus_horizontal(self, -1)

    def action_focus_right(self) -> None:
        focus.focus_horizontal(self, 1)

    def action_focus_up(self) -> None:
        focus.focus_vertical(self, -1)

    def action_focus_down(self) -> None:
        focus.focus_vertical(self, 1)

    def action_deselect(self) -> None:
        if self._leader_active:
            self._deactivate_leader()
            return
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

    # =========================================================================
    # Peek Overlay
    # =========================================================================

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
                iteration = scheduler.get_iteration_count(task.id)
                max_iter = self.kagan_app.config.general.max_iterations
                status = f"🟢 Running (Iter {iteration}/{max_iter})"
            else:
                status = "⚪ Idle"
        else:
            status = "🟢 Session Active" if task.session_active else "⚪ No Active Session"

        scratchpad = await self.ctx.task_service.get_scratchpad(task.id)
        content = scratchpad if scratchpad else "(No scratchpad)"

        overlay.update_content(task.short_id, task.title, status, content)
        x_pos = min(card.region.x + card.region.width + 2, self.size.width - 55)
        y_pos = max(1, card.region.y)
        overlay.show_at(x_pos, y_pos)

    # =========================================================================
    # Leader Key
    # =========================================================================

    def action_activate_leader(self) -> None:
        if self._leader_active:
            return
        self._leader_active = True
        try:
            hint = self.query_one(".leader-hint", Static)
            hint.add_class("visible")
        except NoMatches:
            pass
        self._leader_timer = self.set_timer(LEADER_TIMEOUT, self._leader_timeout)

    def _leader_timeout(self) -> None:
        self._deactivate_leader()

    def _deactivate_leader(self) -> None:
        self._leader_active = False
        if self._leader_timer:
            self._leader_timer.stop()
            self._leader_timer = None
        try:
            hint = self.query_one(".leader-hint", Static)
            hint.remove_class("visible")
        except NoMatches:
            pass

    def _execute_leader_action(self, action_name: str) -> None:
        self._deactivate_leader()
        is_valid, reason = self._validate_action(action_name)
        if not is_valid:
            if reason:
                self.notify(reason, severity="warning")
            return
        action_method = getattr(self, f"action_{action_name}", None)
        if action_method:
            result = action_method()
            if asyncio.iscoroutine(result):
                self.run_worker(result)

    def on_key(self, event: events.Key) -> None:
        if self._leader_active:
            leader_actions = {
                b.key: b.action for b in KANBAN_LEADER_BINDINGS if isinstance(b, Binding)
            }
            if event.key in leader_actions:
                event.prevent_default()
                event.stop()
                self._execute_leader_action(leader_actions[event.key])
            elif event.key == "escape":
                event.prevent_default()
                event.stop()
                self._deactivate_leader()
            else:
                self._deactivate_leader()
            return

        feedback_actions = {
            "delete_task_direct",
            "merge_direct",
            "edit_task",
            "view_details",
            "open_session",
            "start_agent",
            "watch_agent",
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

    # =========================================================================
    # Search
    # =========================================================================

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

    # =========================================================================
    # Task Operations
    # =========================================================================

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
            await self.ctx.task_service.update_fields(task.id, **result)
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
                    merge_readiness=self._merge_readiness.get(card.task_model.id),
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

    async def action_delete_task_direct(self) -> None:
        card = focus.get_focused_card(self)
        if card and card.task_model:
            task = card.task_model
            if self.ctx.merge_service:
                await self.ctx.merge_service.delete_task(task)
            await self._refresh_board()
            self.notify(f"Deleted: {task.title}")
            focus.focus_first_card(self)

    async def action_merge_direct(self) -> None:
        task = self._get_review_task(focus.get_focused_card(self))
        if not task:
            return
        if self.ctx.merge_service and await self.ctx.merge_service.has_no_changes(task):
            success, message = await self.ctx.merge_service.close_exploratory(task)
            if success:
                await self._refresh_board()
                self.notify(f"Closed as exploratory: {task.title}")
            else:
                self.notify(message, severity="error")
            return
        self.notify("Merging... (this may take a few seconds)", severity="information")
        success, message = (
            await self.ctx.merge_service.merge_task(task) if self.ctx.merge_service else (False, "")
        )
        if success:
            await self._refresh_board()
            self.notify(f"Merged: {task.title}", severity="information")
        else:
            self.notify(KanbanScreen._format_merge_failure(task, message), severity="error")

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
                    self._pending_close_task = task
                    title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
                    self.app.push_screen(
                        ConfirmModal(
                            title="Close as Exploratory?",
                            message=f"Close '{title}' with no changes?",
                        ),
                        callback=self._on_close_confirmed,
                    )
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

            # If moving AUTO task out of IN_PROGRESS, clear agent state immediately
            if (
                task_type == TaskType.AUTO
                and status == TaskStatus.IN_PROGRESS
                and new_status != TaskStatus.REVIEW
            ):
                # Clear agent state on UI before moving
                column = self.query_one("#column-in_progress", KanbanColumn)
                column.update_iterations({task.id: ""})
                for c in column.get_cards():
                    if c.task_model and c.task_model.id == task.id:
                        c.is_agent_active = False

            await self.ctx.task_service.move(task.id, new_status)
            await self._refresh_board()
            self.notify(f"Moved #{task.id} to {new_status.value}")
            focus.focus_column(self, new_status)
        else:
            self.notify(f"Already in {'final' if forward else 'first'} status", severity="warning")

    async def _on_merge_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_merge_task:
            task = self._pending_merge_task
            self.notify("Merging... (this may take a few seconds)", severity="information")
            success, message = (
                await self.ctx.merge_service.merge_task(task)
                if self.ctx.merge_service
                else (False, "")
            )
            if success:
                await self._refresh_board()
                self.notify(f"Merged and completed: {task.title}", severity="information")
            else:
                self.notify(KanbanScreen._format_merge_failure(task, message), severity="error")
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
                self.notify(f"Closed as exploratory: {task.title}")
            else:
                self.notify(message, severity="error")
        self._pending_close_task = None

    async def _on_advance_confirmed(self, confirmed: bool | None) -> None:
        if confirmed and self._pending_advance_task:
            task = self._pending_advance_task
            await self.ctx.task_service.update_fields(
                task.id,
                status=TaskStatus.REVIEW,
                merge_failed=False,
                merge_error=None,
                merge_readiness=MergeReadiness.RISK,
            )
            await self.ctx.task_service.append_event(task.id, "review", "Moved to REVIEW")
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

        # Clear agent state immediately on UI to prevent stale indicators
        try:
            column = self.query_one("#column-in_progress", KanbanColumn)
            column.update_iterations({task.id: ""})
            for card in column.get_cards():
                if card.task_model and card.task_model.id == task.id:
                    card.is_agent_active = False
        except Exception:
            pass  # Column might not exist yet

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
            await self.ctx.task_service.update_fields(task.id, **result)
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
                    merge_readiness=self._merge_readiness.get(card.task_model.id),
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

    # =========================================================================
    # Session Operations (inlined from SessionController)
    # =========================================================================

    async def action_open_session(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            return
        task = card.task_model
        if task.status == TaskStatus.REVIEW:
            await self.action_open_review()
            return

        # Only PAIR tasks need manual session opening
        if task.task_type != TaskType.PAIR:
            return

        # Ensure worktree exists
        wt_path = await self.ctx.workspace_service.get_path(task.id)
        if wt_path is None:
            self.notify("Creating worktree...", severity="information")
            base = self.kagan_app.config.general.default_base_branch
            wt_path = await self.ctx.workspace_service.create(task.id, task.title, base)
            self.notify("Worktree created", severity="information")

        # Create session if doesn't exist
        if not await self.ctx.session_service.session_exists(task.id):
            self.notify("Creating session...", severity="information")
            await self.ctx.session_service.create_session(task, wt_path)

        # Show TmuxGatewayModal if not skipped
        if not self.kagan_app.config.ui.skip_tmux_gateway:
            from kagan.ui.modals.tmux_gateway import TmuxGatewayModal

            def on_gateway_result(result: str | None) -> None:
                if result is None:
                    return  # User cancelled
                if result == "skip_future":
                    self.kagan_app.config.ui.skip_tmux_gateway = True
                    cb_result = self._save_tmux_gateway_preference(skip=True)
                    if asyncio.iscoroutine(cb_result):
                        asyncio.create_task(cb_result)
                # Proceed to open tmux session
                self.app.call_later(self._do_open_pair_session, task)

            self.app.push_screen(TmuxGatewayModal(task.id, task.title), on_gateway_result)
            return

        # Skip modal - open directly
        await self._do_open_pair_session(task)

    async def _do_open_pair_session(self, task: Task) -> None:
        """Open the tmux session after modal confirmation."""
        try:
            # Move BACKLOG to IN_PROGRESS if needed
            if task.status == TaskStatus.BACKLOG:
                await self.ctx.task_service.update_fields(task.id, status=TaskStatus.IN_PROGRESS)
                await self._refresh_board()

            # Suspend app and attach to session
            with self.app.suspend():
                await self.ctx.session_service.attach_session(task.id)

            # Check if session still exists after returning from attach
            session_still_exists = await self.ctx.session_service.session_exists(task.id)
            if session_still_exists:
                # User detached, session is still active
                return

            # Session terminated - prompt to move to REVIEW
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
            from kagan.sessions.tmux import TmuxError

            if isinstance(e, TmuxError):
                self.notify(f"Tmux error: {e}", severity="error")

    # =========================================================================
    # Agent Operations (inlined from SessionController)
    # =========================================================================

    async def action_watch_agent(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            return
        task = card.task_model

        # AUTO tasks: Show agent output modal
        if task.task_type == TaskType.AUTO:
            # Show modal if agent is running OR task is IN_PROGRESS/REVIEW
            # (IN_PROGRESS: agent may be starting, REVIEW: can view historical logs)
            is_running = self.ctx.automation_service.is_running(task.id)
            if not is_running and task.status not in (
                TaskStatus.IN_PROGRESS,
                TaskStatus.REVIEW,
            ):
                self.notify("No agent running for this task", severity="warning")
                return

            agent = self.ctx.automation_service.get_running_agent(task.id)
            iteration = self.ctx.automation_service.get_iteration_count(task.id)

            # For REVIEW tasks, also check for review agent and historical logs
            review_agent = None
            is_reviewing_now = False
            historical_logs: dict[str, list[AgentTurn]] = {}

            if task.status == TaskStatus.REVIEW:
                # Get review agent if running
                review_agent = self.ctx.automation_service.get_review_agent(task.id)
                is_reviewing_now = self.ctx.automation_service.is_reviewing(task.id)

                # Load historical logs for both phases
                impl_logs = list(
                    await self.ctx.task_service.get_agent_logs(task.id, log_type="implementation")
                )
                review_logs = list(
                    await self.ctx.task_service.get_agent_logs(task.id, log_type="review")
                )

                if impl_logs:
                    historical_logs["implementation"] = impl_logs
                if review_logs:
                    historical_logs["review"] = review_logs

                # Open modal with review mode
                await self.app.push_screen(
                    AgentOutputModal(
                        task=task,
                        agent=agent,
                        iteration=iteration,
                        review_agent=review_agent,
                        is_reviewing=is_reviewing_now,
                        historical_logs=historical_logs,
                    )
                )
                return

            # For IN_PROGRESS tasks without running agent, check for historical logs
            if agent is None:
                logs = list(
                    await self.ctx.task_service.get_agent_logs(task.id, log_type="implementation")
                )
                if logs:
                    await self.app.push_screen(
                        AgentOutputModal(
                            task=task,
                            agent=None,
                            iteration=iteration,
                            historical_logs={"implementation": logs},
                        )
                    )
                else:
                    self.notify(
                        "No agent logs available yet (agent still starting)", severity="warning"
                    )
                return

            # Normal case: agent is running for IN_PROGRESS task
            await self.app.push_screen(
                AgentOutputModal(
                    task=task,
                    agent=agent,
                    iteration=iteration,
                )
            )
        # PAIR tasks: Attach to tmux session
        else:
            if not await self.ctx.session_service.session_exists(task.id):
                self.notify("No active session for this task", severity="warning")
                return

            # Attach to existing session
            with self.app.suspend():
                await self.ctx.session_service.attach_session(task.id)

    async def action_start_agent(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            return
        task = card.task_model

        # Only AUTO tasks
        if task.task_type == TaskType.PAIR:
            return

        if self.ctx.automation_service.is_running(task.id):
            self.notify(
                "Agent already running for this task (press w to watch)", severity="warning"
            )
            return

        # Move BACKLOG tasks to IN_PROGRESS first
        if task.status == TaskStatus.BACKLOG:
            await self.ctx.task_service.move(task.id, TaskStatus.IN_PROGRESS)
            # Refresh task to get updated status
            refreshed = await self.ctx.task_service.get_task(task.id)
            if refreshed:
                task = refreshed
            await self._refresh_board()

        # Show immediate feedback
        self.notify("Starting agent...", severity="information")

        # Delegate to automation service
        result = self.ctx.automation_service.spawn_for_task(task)
        # Handle both async and sync returns for test compatibility
        if hasattr(result, "__await__"):
            spawned = await result
        else:
            spawned = result

        if spawned:
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

        # Show immediate feedback
        self.notify("Stopping agent...", severity="information")

        result = self.ctx.automation_service.stop_task(task.id)
        # Handle both async and sync returns for test compatibility
        if hasattr(result, "__await__"):
            await result

        self.notify(f"Agent stopped: {task.id[:8]}", severity="information")

    # =========================================================================
    # Screen Navigation
    # =========================================================================

    def action_open_planner(self) -> None:
        self.app.push_screen(PlannerScreen(agent_factory=self.kagan_app._agent_factory))

    async def action_open_settings(self) -> None:
        from kagan.ui.modals import SettingsModal

        config = self.kagan_app.config
        config_path = self.kagan_app.config_path
        result = await self.app.push_screen(SettingsModal(config, config_path))
        if result:
            self.kagan_app.config = self.kagan_app.config.load(config_path)
            self.notify("Settings saved")

    # =========================================================================
    # Review Operations
    # =========================================================================

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
            success, message = await self.ctx.merge_service.close_exploratory(task)
            if success:
                await self._refresh_board()
                self.notify(f"Closed as exploratory: {task.title}")
            else:
                self.notify(message, severity="error")
            return
        self.notify("Merging... (this may take a few seconds)", severity="information")
        success, message = (
            await self.ctx.merge_service.merge_task(task) if self.ctx.merge_service else (False, "")
        )
        if success:
            await self._refresh_board()
            self.notify(f"Merged and completed: {task.title}", severity="information")
        else:
            self.notify(KanbanScreen._format_merge_failure(task, message), severity="error")

    async def action_view_diff(self) -> None:
        task = self._get_review_task(focus.get_focused_card(self))
        if not task:
            return
        worktree = self.ctx.workspace_service
        base = self.kagan_app.config.general.default_base_branch
        diff_text = await worktree.get_diff(task.id, base_branch=base)  # type: ignore[misc]
        title = f"Diff: {task.short_id} {task.title[:NOTIFICATION_TITLE_MAX_LENGTH]}"

        await self.app.push_screen(
            DiffModal(title=title, diff_text=diff_text, task=task),
            callback=lambda result: self._on_diff_result(task, result),
        )

    async def _on_diff_result(self, task: Task, result: str | None) -> None:
        if result == "approve":
            if self.ctx.merge_service and await self.ctx.merge_service.has_no_changes(task):
                success, message = await self.ctx.merge_service.close_exploratory(task)
                if success:
                    await self._refresh_board()
                    self.notify(f"Closed as exploratory: {task.title}")
                else:
                    self.notify(message, severity="error")
                return
            self.notify("Merging... (this may take a few seconds)", severity="information")
            success, message = (
                await self.ctx.merge_service.merge_task(task)
                if self.ctx.merge_service
                else (False, "")
            )
            if success:
                await self._refresh_board()
                self.notify(f"Merged: {task.title}", severity="information")
            else:
                self.notify(KanbanScreen._format_merge_failure(task, message), severity="error")
        elif result == "reject":
            await self._handle_reject_with_feedback(task)

    async def action_open_review(self) -> None:
        task = self._get_review_task(focus.get_focused_card(self))
        if not task:
            return

        agent_config = task.get_agent_config(self.kagan_app.config)
        await self.app.push_screen(
            ReviewModal(
                task=task,
                worktree_manager=self.ctx.workspace_service,
                agent_config=agent_config,
                base_branch=self.kagan_app.config.general.default_base_branch,
                agent_factory=self.kagan_app._agent_factory,
            ),
            callback=self._on_review_result,
        )

    async def _on_review_result(self, result: str | None) -> None:
        task = self._get_review_task(focus.get_focused_card(self))
        if not task:
            return
        if result == "approve":
            self.notify("Merging... (this may take a few seconds)", severity="information")
            success, message = (
                await self.ctx.merge_service.merge_task(task)
                if self.ctx.merge_service
                else (False, "")
            )
            if success:
                await self._refresh_board()
                self.notify(f"Merged and completed: {task.title}", severity="information")
            else:
                self.notify(KanbanScreen._format_merge_failure(task, message), severity="error")
        elif result == "exploratory":
            success, message = (
                await self.ctx.merge_service.close_exploratory(task)
                if self.ctx.merge_service
                else (False, "")
            )
            if success:
                await self._refresh_board()
                self.notify(f"Closed as exploratory: {task.title}")
            else:
                self.notify(message, severity="error")
        elif result == "reject":
            await self._handle_reject_with_feedback(task)

    @staticmethod
    def _format_merge_failure(task: Task, message: str) -> str:
        task_type = task.task_type
        if task_type == TaskType.AUTO:
            return f"Merge failed (AUTO): {message}"
        return f"Merge failed (PAIR): {message}"

    async def _handle_reject_with_feedback(self, task: Task) -> None:
        task_type = task.task_type
        if task_type == TaskType.AUTO:
            await self.app.push_screen(
                RejectionInputModal(task.title),
                callback=lambda result: self._apply_rejection_result(task, result),
            )
        else:
            await self.ctx.task_service.move(task.id, TaskStatus.IN_PROGRESS)
            await self._refresh_board()
            self.notify(f"Moved back to IN_PROGRESS: {task.title}")

    async def _apply_rejection_result(self, task: Task, result: tuple[str, str] | None) -> None:
        if self.ctx.merge_service is None:
            return
        if result is None:
            await self.ctx.merge_service.apply_rejection_feedback(task, None, "shelve")
        else:
            feedback, action = result
            await self.ctx.merge_service.apply_rejection_feedback(task, feedback, action)
        await self._refresh_board()
        if result is None:
            self.notify(f"Shelved: {task.title}")
        elif result[1] == "retry":
            self.notify(f"Retrying: {task.title}")
        else:
            self.notify(f"Staged for manual restart: {task.title}")

    # =========================================================================
    # Config Persistence
    # =========================================================================

    async def _save_tmux_gateway_preference(self, skip: bool = True) -> None:
        """Save tmux gateway preference to config."""
        try:
            await self.kagan_app.config.update_ui_preferences(
                self.kagan_app.config_path,
                skip_tmux_gateway=skip,
            )
        except Exception as e:
            self.notify(f"Failed to save preference: {e}", severity="error")

    # =========================================================================
    # Message Handlers
    # =========================================================================

    def on_task_card_selected(self, message: TaskCard.Selected) -> None:
        self.action_view_details()
