from __future__ import annotations

from contextlib import suppress
from typing import TYPE_CHECKING

from textual.css.query import NoMatches
from textual.widgets import Static

from kagan.constants import (
    COLUMN_ORDER,
    MIN_SCREEN_HEIGHT,
    MIN_SCREEN_WIDTH,
    NOTIFICATION_TITLE_MAX_LENGTH,
    STATUS_LABELS,
)
from kagan.core.models.enums import CardIndicator, TaskStatus, TaskType
from kagan.ui.screens.kanban import focus
from kagan.ui.screens.kanban.hints import build_kanban_hints
from kagan.ui.widgets.card import TaskCard
from kagan.ui.widgets.column import KanbanColumn
from kagan.ui.widgets.keybinding_hint import KanbanHintBar
from kagan.ui.widgets.offline_banner import OfflineBanner
from kagan.ui.widgets.search_bar import SearchBar

if TYPE_CHECKING:
    from collections.abc import Sequence

    from kagan.core.models.entities import Task
    from kagan.ui.screens.kanban.screen import KanbanScreen


class KanbanBoardController:
    def __init__(self, screen: KanbanScreen) -> None:
        self.screen = screen

    def check_agent_health(self) -> None:
        if not self.screen.ctx.agent_health.is_available():
            self.screen._agent_offline = True
            message = self.screen.ctx.agent_health.get_status_message()
            banner = OfflineBanner(message=message)
            self.screen.mount(banner, before=self.screen.query_one(".board-container"))
        else:
            self.screen._agent_offline = False

    def cleanup_on_unmount(self) -> None:
        self.screen._pending_delete_task = None
        self.screen._pending_merge_task = None
        self.screen._pending_advance_task = None
        self.screen._pending_auto_move_task = None
        self.screen._pending_auto_move_status = None
        self.screen._editing_task_id = None
        self.screen._filtered_tasks = None
        self.screen._merge_failed_tasks.clear()
        if self.screen._refresh_timer:
            self.screen._refresh_timer.stop()
            self.screen._refresh_timer = None

    async def reset_for_repo_change(self) -> None:
        with suppress(NoMatches):
            search_bar = self.screen.query_one("#search-bar", SearchBar)
            if search_bar.is_visible:
                search_bar.hide()
                search_bar.clear()
        self.screen._filtered_tasks = None
        self.screen._task_hashes.clear()
        self.screen._tasks = []
        await self.refresh_and_sync()
        focus.focus_first_card(self.screen)

    def sync_agent_states(self) -> None:
        scheduler = self.screen.ctx.automation_service
        running_tasks = scheduler.running_tasks
        indicators: dict[str, CardIndicator] = {}

        for tid in running_tasks:
            indicators[tid] = (
                CardIndicator.REVIEWING if scheduler.is_reviewing(tid) else CardIndicator.RUNNING
            )

        for task in self.screen._tasks:
            if task.id in indicators:
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
        if is_active is None:
            is_active = indicator in {CardIndicator.RUNNING, CardIndicator.REVIEWING}
        for column in self.screen.query(KanbanColumn):
            for card in column.get_cards():
                if card.task_model is None or card.task_model.id != task_id:
                    continue
                card.is_agent_active = is_active
                card.indicator = indicator
                return

    def check_screen_size(self) -> None:
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

        new_tasks = await self.screen.ctx.task_service.list_tasks(
            project_id=self.screen.ctx.active_project_id
        )
        display_tasks: Sequence[Task] = (
            self.screen._filtered_tasks if self.screen._filtered_tasks is not None else new_tasks
        )

        old_status_by_id = {task.id: task.status for task in self.screen._tasks}

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
        changed_ids = {
            tid for tid, h in new_hashes.items() if self.screen._task_hashes.get(tid) != h
        }
        deleted_ids = set(self.screen._task_hashes.keys()) - set(new_hashes.keys())

        if changed_ids or deleted_ids or self.screen._task_hashes == {}:
            self.notify_status_changes(new_tasks, old_status_by_id, changed_ids)
            self.screen._tasks = new_tasks
            self.screen._task_hashes = new_hashes

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
                    column = self.screen.query_one(f"#column-{status.value.lower()}", KanbanColumn)
                    column.update_tasks([t for t in display_tasks if t.status == status])

            with suppress(NoMatches):
                self.screen.header.update_count(len(self.screen._tasks))
            self.update_review_queue_hint()
            self.update_keybinding_hints()
            self.screen.refresh_bindings()
            if focused_task_id:
                self.restore_focus(focused_task_id)

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
        if self.screen._refresh_timer:
            self.screen._refresh_timer.stop()
        self.screen._refresh_timer = self.screen.set_timer(0.15, self.run_refresh)

    def run_refresh(self) -> None:
        self.screen._refresh_timer = None
        if not self.screen.is_mounted:
            return
        self.screen.run_worker(self.refresh_and_sync())

    def update_review_queue_hint(self) -> None:
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

        card = focus.get_focused_card(self.screen)
        if not card or not card.task_model:
            hints = build_kanban_hints(None, None)
        else:
            hints = build_kanban_hints(card.task_model.status, card.task_model.task_type)

        hint_bar.show_kanban_hints(hints.navigation, hints.actions, hints.global_hints)
