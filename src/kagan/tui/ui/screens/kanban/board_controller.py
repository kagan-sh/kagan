from __future__ import annotations

from contextlib import suppress
from dataclasses import dataclass
from enum import StrEnum
from typing import TYPE_CHECKING, Any

from sqlalchemy.exc import OperationalError
from textual.css.query import NoMatches
from textual.widgets import Static

from kagan.core.adapters.db.repositories.base import RepositoryClosing
from kagan.core.constants import (
    COLUMN_ORDER,
    MIN_SCREEN_HEIGHT,
    MIN_SCREEN_WIDTH,
    NOTIFICATION_TITLE_MAX_LENGTH,
    STATUS_LABELS,
)
from kagan.core.models.enums import CardIndicator, TaskStatus, TaskType
from kagan.tui.ui.screens.kanban.hints import build_kanban_hints
from kagan.tui.ui.utils import state_attr, state_timestamp
from kagan.tui.ui.widgets.card import TaskCard
from kagan.tui.ui.widgets.column import KanbanColumn
from kagan.tui.ui.widgets.keybinding_hint import KanbanHintBar
from kagan.tui.ui.widgets.offline_banner import OfflineBanner

if TYPE_CHECKING:
    from collections.abc import Sequence
    from typing import Protocol

    from kagan.core.adapters.db.schema import Task
    from kagan.tui.ui.screens.kanban.screen import KanbanScreen

    class BoardTaskLike(Protocol):
        @property
        def id(self) -> str: ...

        @property
        def status(self) -> TaskStatus: ...

        @property
        def title(self) -> str: ...

        @property
        def description(self) -> str | None: ...

        @property
        def updated_at(self) -> object: ...


BOARD_SYNC_FAST_INTERVAL_SECONDS = 1.0
BOARD_SYNC_IDLE_INTERVAL_SECONDS = 3.0
BOARD_SYNC_FAST_TICKS_AFTER_ACTIVITY = 5


class BoardSyncState(StrEnum):
    FAST = "fast"
    IDLE = "idle"


@dataclass(frozen=True)
class BoardSyncTransition:
    state: BoardSyncState
    fast_ticks_remaining: int


@dataclass(frozen=True)
class BoardTaskDiff:
    new_hashes: dict[str, int]
    changed_ids: set[str]
    deleted_ids: set[str]
    affected_statuses: set[TaskStatus]
    has_task_mutation: bool


@dataclass(frozen=True)
class BoardRefreshModel:
    new_tasks: list[Task]
    old_status_by_id: dict[str, TaskStatus]
    diff: BoardTaskDiff
    display_tasks_by_status: dict[TaskStatus, list[Task]]
    backlog_blocked_count: int


def transition_board_sync_state(
    *,
    current_state: BoardSyncState,
    fast_ticks_remaining: int,
    has_activity: bool,
    fast_ticks_after_activity: int = BOARD_SYNC_FAST_TICKS_AFTER_ACTIVITY,
) -> BoardSyncTransition:
    if has_activity:
        return BoardSyncTransition(BoardSyncState.FAST, fast_ticks_after_activity)
    if current_state is BoardSyncState.FAST and fast_ticks_remaining > 1:
        return BoardSyncTransition(BoardSyncState.FAST, fast_ticks_remaining - 1)
    return BoardSyncTransition(BoardSyncState.IDLE, 0)


def task_content_hash(task: BoardTaskLike) -> int:
    return hash((task.status.value, task.title, task.description, task.updated_at))


def compute_board_task_diff(
    *,
    previous_hashes: dict[str, int],
    previous_status_by_id: dict[str, TaskStatus],
    new_tasks: Sequence[BoardTaskLike],
) -> BoardTaskDiff:
    new_hashes = {task.id: task_content_hash(task) for task in new_tasks}
    changed_ids = {
        task_id
        for task_id, task_hash in new_hashes.items()
        if previous_hashes.get(task_id) != task_hash
    }
    deleted_ids = set(previous_hashes) - set(new_hashes)

    has_task_mutation = bool(changed_ids or deleted_ids or not previous_hashes)
    affected_statuses: set[TaskStatus] = set()
    if has_task_mutation:
        for task in new_tasks:
            if task.id not in changed_ids:
                continue
            affected_statuses.add(task.status)
            old_status = previous_status_by_id.get(task.id)
            if old_status is not None and old_status is not task.status:
                affected_statuses.add(old_status)
        if deleted_ids:
            affected_statuses = set(COLUMN_ORDER)

    return BoardTaskDiff(
        new_hashes=new_hashes,
        changed_ids=changed_ids,
        deleted_ids=deleted_ids,
        affected_statuses=affected_statuses,
        has_task_mutation=has_task_mutation,
    )


def group_tasks_by_status[T: BoardTaskLike](
    display_tasks: Sequence[T],
    *,
    blocked_backlog_timestamps: dict[str, float],
) -> dict[TaskStatus, list[T]]:
    grouped: dict[TaskStatus, list[T]] = {status: [] for status in COLUMN_ORDER}
    for task in display_tasks:
        grouped[task.status].append(task)

    backlog_tasks = grouped[TaskStatus.BACKLOG]
    if not backlog_tasks:
        return grouped

    original_index = {task.id: index for index, task in enumerate(backlog_tasks)}

    def _backlog_sort_key(task: T) -> tuple[int, float, int]:
        blocked_timestamp = blocked_backlog_timestamps.get(task.id)
        if blocked_timestamp is None:
            return (1, 0.0, original_index[task.id])
        return (0, blocked_timestamp, original_index[task.id])

    grouped[TaskStatus.BACKLOG] = sorted(backlog_tasks, key=_backlog_sort_key)
    return grouped


def count_blocked_backlog_tasks(
    display_tasks: Sequence[BoardTaskLike],
    *,
    blocked_backlog_ids: set[str],
) -> int:
    return sum(
        1
        for task in display_tasks
        if task.status is TaskStatus.BACKLOG and task.id in blocked_backlog_ids
    )


class KanbanBoardController:
    def __init__(self, screen: KanbanScreen) -> None:
        self.screen = screen

    @staticmethod
    def _runtime_attr(runtime_view: object | None, name: str, default: Any = None) -> Any:
        return state_attr(runtime_view, name, default)

    @classmethod
    def _runtime_bool(cls, runtime_view: object | None, name: str) -> bool:
        return bool(cls._runtime_attr(runtime_view, name, False))

    @classmethod
    def _runtime_timestamp(cls, runtime_view: object | None, name: str) -> float:
        del cls
        return state_timestamp(runtime_view, name)

    def check_agent_health(self) -> None:
        """Check agent health."""
        if not self.screen.ctx.api.is_agent_available():
            self.screen._agent_offline = True
            message = self.screen.ctx.api.get_agent_status_message()
            banner = OfflineBanner(message=message)
            self.screen.mount(banner, before=self.screen.query_one(".board-container"))
        else:
            self.screen._agent_offline = False

    def cleanup_on_unmount(self) -> None:
        self.screen._ui_state.clear_all()
        self.screen._merge_failed_tasks.clear()
        if self.screen._refresh_timer:
            self.screen._refresh_timer.stop()
            self.screen._refresh_timer = None
        self.stop_background_sync()

    def start_background_sync(self) -> None:
        self._set_fast_sync_window()

    def stop_background_sync(self) -> None:
        if self.screen._board_sync_timer:
            self.screen._board_sync_timer.stop()
            self.screen._board_sync_timer = None
        self.screen._board_sync_interval_seconds = None
        self.screen._board_fast_ticks_remaining = 0

    def _set_fast_sync_window(self) -> None:
        transition = transition_board_sync_state(
            current_state=self._sync_state(),
            fast_ticks_remaining=self.screen._board_fast_ticks_remaining,
            has_activity=True,
        )
        self._apply_sync_transition(transition)

    def _sync_state(self) -> BoardSyncState:
        if self.screen._board_sync_interval_seconds == BOARD_SYNC_FAST_INTERVAL_SECONDS:
            return BoardSyncState.FAST
        return BoardSyncState.IDLE

    def _apply_sync_transition(self, transition: BoardSyncTransition) -> None:
        self.screen._board_fast_ticks_remaining = transition.fast_ticks_remaining
        if transition.state is BoardSyncState.FAST:
            self._set_background_sync_interval(BOARD_SYNC_FAST_INTERVAL_SECONDS)
            return
        self._set_background_sync_interval(BOARD_SYNC_IDLE_INTERVAL_SECONDS)

    def _set_background_sync_interval(self, interval_seconds: float) -> None:
        if (
            self.screen._board_sync_interval_seconds == interval_seconds
            and self.screen._board_sync_timer
        ):
            return
        if self.screen._board_sync_timer:
            self.screen._board_sync_timer.stop()
        self.screen._board_sync_interval_seconds = interval_seconds
        self.screen._board_sync_timer = self.screen.set_interval(
            interval_seconds,
            self._background_sync_tick,
        )

    def _background_sync_tick(self) -> None:
        running_tasks = self.screen.ctx.api.get_running_task_ids()
        has_auto_in_progress = any(
            task.task_type == TaskType.AUTO and task.status == TaskStatus.IN_PROGRESS
            for task in self.screen._tasks
        )
        transition = transition_board_sync_state(
            current_state=self._sync_state(),
            fast_ticks_remaining=self.screen._board_fast_ticks_remaining,
            has_activity=bool(running_tasks) or has_auto_in_progress,
        )
        self._apply_sync_transition(transition)

        # Event-driven refreshes remain primary; avoid stacking duplicate work.
        if self.screen._refresh_timer is not None:
            return
        if self._has_active_refresh_worker():
            return
        self.run_refresh()

    def _has_active_refresh_worker(self) -> bool:
        return any(
            worker.node == self.screen
            and worker.group == "kanban-refresh"
            and not worker.is_finished
            for worker in self.screen.workers
        )

    async def reset_for_repo_change(self) -> None:
        self.screen.invalidate_search_requests(cancel_workers=True)
        if self.screen.search_visible:
            self.screen.search_visible = False
        self.screen._ui_state.filtered_tasks = None
        self.screen._task_hashes.clear()
        self.screen._tasks = []
        await self.refresh_and_sync()
        self.screen.focus_first_card()

    def sync_agent_states(self) -> None:
        """Synchronize agent states."""
        api = self.screen.ctx.api
        running_tasks = api.get_running_task_ids()
        indicators = {}
        for tid in running_tasks:
            view = api.get_runtime_view(tid)
            indicators[tid] = (
                CardIndicator.REVIEWING
                if self._runtime_bool(view, "is_reviewing")
                else CardIndicator.RUNNING
            )
        for task in self.screen._tasks:
            if task.id in indicators:
                continue
            runtime = api.get_runtime_view(task.id)
            if self._runtime_bool(runtime, "is_blocked"):
                indicators[task.id] = CardIndicator.BLOCKED
                continue
            if task.id in self.screen._merge_failed_tasks:
                indicators[task.id] = CardIndicator.FAILED
            elif task.status == TaskStatus.IN_PROGRESS and task.task_type == TaskType.AUTO:
                indicators[task.id] = CardIndicator.IDLE
            elif task.status == TaskStatus.DONE:
                indicators[task.id] = CardIndicator.PASSED

        for column in self.screen.query(KanbanColumn):
            column.update_active_states(running_tasks, indicators)

    def set_card_indicator(
        self,
        task_id: str,
        indicator: CardIndicator,
        *,
        is_active: bool | None = None,
    ) -> None:
        """Set card indicator."""
        if is_active is None:
            is_active = indicator in {CardIndicator.RUNNING, CardIndicator.REVIEWING}
        for column in self.screen.query(KanbanColumn):
            for card in column.get_cards():
                if card.task_model is None or card.task_model.id != task_id:
                    continue
                card.is_agent_active = is_active
                card.indicator = indicator
                return

    def _blocked_backlog_metadata(
        self,
        display_tasks: Sequence[Task],
    ) -> tuple[set[str], dict[str, float]]:
        blocked_ids: set[str] = set()
        blocked_timestamps: dict[str, float] = {}
        for task in display_tasks:
            if task.status is not TaskStatus.BACKLOG or task.task_type is not TaskType.AUTO:
                continue
            runtime = self.screen.ctx.api.get_runtime_view(task.id)
            if not self._runtime_bool(runtime, "is_blocked"):
                continue
            blocked_ids.add(task.id)
            blocked_timestamps[task.id] = self._runtime_timestamp(runtime, "blocked_at")
        return blocked_ids, blocked_timestamps

    def _build_refresh_model(self, new_tasks: list[Task]) -> BoardRefreshModel:
        display_tasks: Sequence[Task] = (
            self.screen._ui_state.filtered_tasks
            if self.screen._ui_state.filtered_tasks is not None
            else new_tasks
        )
        blocked_ids, blocked_timestamps = self._blocked_backlog_metadata(display_tasks)
        display_tasks_by_status = group_tasks_by_status(
            display_tasks,
            blocked_backlog_timestamps=blocked_timestamps,
        )
        backlog_blocked_count = count_blocked_backlog_tasks(
            display_tasks,
            blocked_backlog_ids=blocked_ids,
        )
        old_status_by_id = {task.id: task.status for task in self.screen._tasks}
        diff = compute_board_task_diff(
            previous_hashes=self.screen._task_hashes,
            previous_status_by_id=old_status_by_id,
            new_tasks=new_tasks,
        )
        return BoardRefreshModel(
            new_tasks=new_tasks,
            old_status_by_id=old_status_by_id,
            diff=diff,
            display_tasks_by_status=display_tasks_by_status,
            backlog_blocked_count=backlog_blocked_count,
        )

    def _update_columns(
        self,
        statuses: set[TaskStatus],
        *,
        display_tasks_by_status: dict[TaskStatus, list[Task]],
        backlog_blocked_count: int,
    ) -> None:
        for status in statuses:
            with suppress(NoMatches):
                column = self.screen.query_one(f"#column-{status.value.lower()}", KanbanColumn)
                column.update_tasks(display_tasks_by_status.get(status, []))
                if status is TaskStatus.BACKLOG:
                    column.update_blocked_count(backlog_blocked_count)

    def _refresh_backlog_projection(
        self,
        *,
        display_tasks_by_status: dict[TaskStatus, list[Task]],
        backlog_blocked_count: int,
    ) -> None:
        with suppress(NoMatches):
            backlog_column = self.screen.query_one("#column-backlog", KanbanColumn)
            backlog_column.update_tasks(display_tasks_by_status[TaskStatus.BACKLOG])
            backlog_column.update_blocked_count(backlog_blocked_count)

    def _apply_refresh_model(
        self,
        model: BoardRefreshModel,
        *,
        focused_task_id: str | None,
    ) -> None:
        has_search_filter = self.screen._ui_state.filtered_tasks is not None

        if model.diff.has_task_mutation:
            self.notify_status_changes(
                model.new_tasks,
                model.old_status_by_id,
                model.diff.changed_ids,
            )
            self.screen._tasks = model.new_tasks
            self.screen._task_hashes = model.diff.new_hashes

            affected_statuses = model.diff.affected_statuses
            if not model.old_status_by_id:
                affected_statuses = set(COLUMN_ORDER)
            if has_search_filter:
                affected_statuses = set(COLUMN_ORDER)
            self._update_columns(
                affected_statuses,
                display_tasks_by_status=model.display_tasks_by_status,
                backlog_blocked_count=model.backlog_blocked_count,
            )

            # Runtime projection can change independently of task DB fields.
            # Keep backlog ordering/blocked badge synced even when hash-based diff
            # touches other statuses only.
            if TaskStatus.BACKLOG not in affected_statuses:
                self._refresh_backlog_projection(
                    display_tasks_by_status=model.display_tasks_by_status,
                    backlog_blocked_count=model.backlog_blocked_count,
                )

            with suppress(NoMatches):
                self.screen.header.update_count(len(self.screen._tasks))
            self.update_review_queue_hint()
            self.update_keybinding_hints()
            self.screen.refresh_bindings()
            if focused_task_id:
                self.restore_focus(focused_task_id)
            return

        if has_search_filter:
            self._update_columns(
                set(COLUMN_ORDER),
                display_tasks_by_status=model.display_tasks_by_status,
                backlog_blocked_count=model.backlog_blocked_count,
            )
            return

        self._refresh_backlog_projection(
            display_tasks_by_status=model.display_tasks_by_status,
            backlog_blocked_count=model.backlog_blocked_count,
        )

    def check_screen_size(self) -> None:
        """Check screen size."""
        size = self.screen.app.size
        if size.width < MIN_SCREEN_WIDTH or size.height < MIN_SCREEN_HEIGHT:
            self.screen.add_class("too-small")
        else:
            self.screen.remove_class("too-small")

    async def refresh_board(self) -> None:
        if not self.screen.is_mounted:
            return
        focused_task_id = None
        focused = self.screen.app.focused
        if isinstance(focused, TaskCard) and focused.task_model:
            focused_task_id = focused.task_model.id

        # Reconcile BEFORE list_tasks so runtime snapshots are fresh
        prev_auto_ids = [t.id for t in self.screen._tasks if t.task_type == TaskType.AUTO]
        if prev_auto_ids:
            try:
                with suppress(ConnectionError, RepositoryClosing, OperationalError):
                    await self.screen.ctx.api.reconcile_running_tasks(prev_auto_ids)
            except ValueError as exc:
                # aiosqlite can raise ValueError("Connection closed") during shutdown/cancellation.
                if str(exc) != "Connection closed":
                    raise

        try:
            new_tasks = await self.screen.ctx.api.list_tasks(
                project_id=self.screen.ctx.active_project_id
            )
        except (ConnectionError, RepositoryClosing, OperationalError):
            return
        except ValueError as exc:
            # aiosqlite can raise ValueError("Connection closed") during shutdown/cancellation.
            if str(exc) != "Connection closed":
                raise
            return

        model = self._build_refresh_model(new_tasks)
        self._apply_refresh_model(model, focused_task_id=focused_task_id)
        self.sync_agent_states()

    def notify_status_changes(
        self,
        new_tasks: list[Task],
        old_status_by_id: dict[str, TaskStatus],
        changed_ids: set[str],
    ) -> None:
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
            self.screen.notify(
                f"#{task.short_id} {title}: {old_label} -> {new_label}",
                severity="information",
            )

        remaining = len(transitions) - max_toasts
        if remaining > 0:
            self.screen.notify(
                f"{remaining} more task(s) changed status",
                severity="information",
            )

    def restore_focus(self, task_id: str) -> None:
        for column in self.screen.query(KanbanColumn):
            for card in column.get_cards():
                if card.task_model and card.task_model.id == task_id:
                    card.focus()
                    return

    async def refresh_and_sync(self) -> None:
        await self.refresh_board()

    def schedule_refresh(self) -> None:
        self._set_fast_sync_window()
        if self.screen._refresh_timer:
            self.screen._refresh_timer.stop()
        self.screen._refresh_timer = self.screen.set_timer(0.15, self.run_refresh)

    def run_refresh(self) -> None:
        self.screen._refresh_timer = None
        if not self.screen.is_mounted:
            return
        self.screen.run_worker(
            self.refresh_and_sync(),
            group="kanban-refresh",
            exclusive=True,
            exit_on_error=False,
        )

    def update_review_queue_hint(self) -> None:
        """Update review queue hint."""
        try:
            hint = self.screen.query_one("#review-queue-hint", Static)
        except NoMatches:
            return
        review_count = sum(1 for task in self.screen._tasks if task.status == TaskStatus.REVIEW)
        if review_count > 1:
            hint.update("Hint: multiple tasks are in REVIEW. Merging in order reduces conflicts.")
            hint.add_class("visible")
        else:
            hint.update("")
            hint.remove_class("visible")

    def update_keybinding_hints(self) -> None:
        try:
            hint_bar = self.screen.query_one("#kanban-hint-bar", KanbanHintBar)
        except NoMatches:
            return

        card = self.screen.get_focused_card()
        if not card or not card.task_model:
            hints = build_kanban_hints(None, None)
        else:
            hints = build_kanban_hints(
                card.task_model.status,
                card.task_model.task_type,
            )

        hint_bar.show_kanban_hints(hints.navigation, hints.actions, hints.global_hints)
