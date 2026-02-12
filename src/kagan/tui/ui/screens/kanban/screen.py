"""Main Kanban board screen."""

from __future__ import annotations

import asyncio
from contextlib import suppress
from typing import TYPE_CHECKING

from textual import getters, on
from textual.binding import Binding
from textual.containers import Container, Horizontal
from textual.css.query import NoMatches
from textual.reactive import var
from textual.widgets import Static

from kagan.core.constants import (
    COLUMN_ORDER,
    MIN_SCREEN_HEIGHT,
    MIN_SCREEN_WIDTH,
)
from kagan.core.models.enums import TaskStatus, TaskType
from kagan.tui.keybindings import KANBAN_BINDINGS
from kagan.tui.ui.modals.description_editor import DescriptionEditorModal
from kagan.tui.ui.screen_result import await_screen_result
from kagan.tui.ui.screens.base import KaganScreen
from kagan.tui.ui.screens.kanban.board_controller import KanbanBoardController
from kagan.tui.ui.screens.kanban.commands import (
    KANBAN_ACTIONS,
    KanbanActionId,
    KanbanCommandProvider,
    get_kanban_action,
)
from kagan.tui.ui.screens.kanban.review_controller import KanbanReviewController
from kagan.tui.ui.screens.kanban.session_controller import KanbanSessionController
from kagan.tui.ui.screens.kanban.state import KanbanUiState
from kagan.tui.ui.screens.kanban.task_controller import KanbanTaskController
from kagan.tui.ui.screens.planner import PlannerScreen
from kagan.tui.ui.utils import copy_with_notification
from kagan.tui.ui.widgets.column import KanbanColumn
from kagan.tui.ui.widgets.header import KaganHeader
from kagan.tui.ui.widgets.keybinding_hint import KanbanHintBar
from kagan.tui.ui.widgets.offline_banner import OfflineBanner
from kagan.tui.ui.widgets.peek_overlay import PeekOverlay
from kagan.tui.ui.widgets.search_bar import SearchBar

if TYPE_CHECKING:
    from collections.abc import Awaitable, Callable

    from textual import events
    from textual.app import ComposeResult
    from textual.timer import Timer

    from kagan.core.adapters.db.schema import Task
    from kagan.tui.ui.widgets.card import TaskCard

SIZE_WARNING_MESSAGE = (
    f"Terminal too small\n\n"
    f"Minimum size: {MIN_SCREEN_WIDTH}x{MIN_SCREEN_HEIGHT}\n"
    f"Please resize your terminal"
)

KANBAN_DONE_BLOCKED_ACTIONS = frozenset(
    {
        KanbanActionId.EDIT_TASK,
        KanbanActionId.MOVE_FORWARD,
        KanbanActionId.MOVE_BACKWARD,
    }
)
KANBAN_REVIEW_ONLY_ACTIONS = frozenset(
    {
        KanbanActionId.MERGE,
        KanbanActionId.MERGE_DIRECT,
        KanbanActionId.VIEW_DIFF,
        KanbanActionId.OPEN_REVIEW,
        KanbanActionId.REBASE,
    }
)
KANBAN_AUTO_ONLY_ACTIONS = frozenset(
    {
        KanbanActionId.START_AGENT,
        KanbanActionId.STOP_AGENT,
    }
)
KANBAN_ACTIONS_WITH_KEY_FEEDBACK = frozenset(
    {
        KanbanActionId.DELETE_TASK_DIRECT,
        KanbanActionId.MERGE_DIRECT,
        KanbanActionId.REBASE,
        KanbanActionId.EDIT_TASK,
        KanbanActionId.VIEW_DETAILS,
        KanbanActionId.OPEN_SESSION,
        KanbanActionId.START_AGENT,
        KanbanActionId.STOP_AGENT,
        KanbanActionId.VIEW_DIFF,
        KanbanActionId.OPEN_REVIEW,
    }
)


def _as_kanban_action_id(action: str | KanbanActionId | None) -> KanbanActionId | None:
    if action is None:
        return None
    try:
        return KanbanActionId(action)
    except ValueError:
        return None


class KanbanScreen(KaganScreen):
    """Main Kanban board screen with 4 columns."""

    COMMANDS = {KanbanCommandProvider}
    BINDINGS = KANBAN_BINDINGS
    search_visible: var[bool] = var(False, init=False)

    header = getters.query_one(KaganHeader)

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self._tasks: list[Task] = []
        self._ui_state = KanbanUiState()
        self._refresh_timer: Timer | None = None
        self._board_sync_timer: Timer | None = None
        self._board_sync_interval_seconds: float | None = None
        self._board_fast_ticks_remaining: int = 0
        self._task_hashes: dict[str, int] = {}
        self._agent_offline: bool = False
        self._merge_failed_tasks: set[str] = set()
        self._search_request_id: int = 0
        self._board = KanbanBoardController(self)
        self._review = KanbanReviewController(self)
        self._session = KanbanSessionController(self)
        self._task_controller = KanbanTaskController(self)

    _TASK_REQUIRED_ACTIONS = frozenset(item.action for item in KANBAN_ACTIONS if item.requires_task)
    _AGENT_REQUIRED_ACTIONS = frozenset(
        item.action for item in KANBAN_ACTIONS if item.requires_agent
    )

    def _validate_action(self, action: str | KanbanActionId) -> tuple[bool, str | None]:
        action_id = _as_kanban_action_id(action)
        if self._agent_offline and action_id in self._AGENT_REQUIRED_ACTIONS:
            return (False, "Agent unavailable (offline mode)")

        card = self.get_focused_card()
        task = card.task_model if card else None

        if not task:
            if action_id in self._TASK_REQUIRED_ACTIONS:
                return (False, "No task selected")
            return (True, None)

        status = task.status
        task_type = task.task_type

        if action_id in KANBAN_DONE_BLOCKED_ACTIONS and status is TaskStatus.DONE:
            message = "Done tasks cannot be edited. Use [y] to duplicate."
            if action_id in {KanbanActionId.MOVE_FORWARD, KanbanActionId.MOVE_BACKWARD}:
                message = "Done tasks cannot be moved. Use [y] to duplicate."
            return (False, message)

        if action_id in KANBAN_REVIEW_ONLY_ACTIONS:
            if status != TaskStatus.REVIEW:
                return (False, f"Only available for REVIEW tasks (current: {status.value})")
            return (True, None)

        if action_id in KANBAN_AUTO_ONLY_ACTIONS:
            if task_type is not TaskType.AUTO:
                return (False, "Only available for AUTO tasks")
            if action_id is KanbanActionId.STOP_AGENT and not self._is_runtime_running(task.id):
                return (False, "No agent running for this task")
            return (True, None)

        return (True, None)

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool | None:
        is_valid, _ = self._validate_action(action)
        return True if is_valid else None

    def _runtime_view(self, task_id: str):
        return self.ctx.api.get_runtime_view(task_id)

    def _is_runtime_running(self, task_id: str) -> bool:
        runtime_view = self._runtime_view(task_id)
        return runtime_view.is_running if runtime_view is not None else False

    def _runtime_run_count(self, task_id: str) -> int:
        runtime_view = self._runtime_view(task_id)
        return runtime_view.run_count if runtime_view is not None else 0

    def get_columns(self) -> list[KanbanColumn]:
        return [
            self.query_one(f"#column-{status.value.lower()}", KanbanColumn)
            for status in COLUMN_ORDER
        ]

    def get_focused_card(self) -> TaskCard | None:
        from kagan.tui.ui.widgets.card import TaskCard

        focused = self.app.focused
        return focused if isinstance(focused, TaskCard) else None

    def focus_first_card(self) -> None:
        for column in self.get_columns():
            if column.focus_first_card():
                return

    def focus_column(self, status: TaskStatus) -> None:
        column = self.query_one(f"#column-{status.value.lower()}", KanbanColumn)
        column.focus_first_card()

    def focus_horizontal(self, direction: int) -> None:
        card = self.get_focused_card()
        columns = self.get_columns()
        if not card or not card.task_model:
            self.focus_first_card()
            return

        column_index = next(
            (
                index
                for index, status in enumerate(COLUMN_ORDER)
                if status == card.task_model.status
            ),
            -1,
        )
        card_index = columns[column_index].get_focused_card_index() or 0
        target_column_index = column_index + direction
        while 0 <= target_column_index < len(COLUMN_ORDER):
            cards = columns[target_column_index].get_cards()
            if cards:
                columns[target_column_index].focus_card(min(card_index, len(cards) - 1))
                return
            target_column_index += direction

    def focus_vertical(self, direction: int) -> None:
        card = self.get_focused_card()
        if not card or not card.task_model:
            self.focus_first_card()
            return

        status = card.task_model.status
        status_str = status.value if isinstance(status, TaskStatus) else status
        column = self.query_one(f"#column-{status_str.lower()}", KanbanColumn)
        index = column.get_focused_card_index()
        cards = column.get_cards()
        if index is None:
            return
        new_index = index + direction
        if 0 <= new_index < len(cards):
            column.focus_card(new_index)

    def compose(self) -> ComposeResult:
        yield KaganHeader(task_count=0)
        yield SearchBar(id="search-bar").data_bind(is_visible=KanbanScreen.search_visible)
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
        self._board.check_screen_size()
        self._board.check_agent_health()
        await self.sync_header_context(self.header)
        await self._board.refresh_board()
        self.focus_first_card()
        self.kagan_app.task_changed_signal.subscribe(self, self._on_task_changed)
        self._board.start_background_sync()
        self._board.sync_agent_states()
        from kagan.tui.ui.widgets.header import _get_git_branch

        if self.ctx.active_repo_id is None:
            self.header.update_branch("")
            return
        branch = await _get_git_branch(self.kagan_app.project_root)
        self.header.update_branch(branch)

    def on_unmount(self) -> None:
        with suppress(Exception):
            self.kagan_app.task_changed_signal.unsubscribe(self)
        self.invalidate_search_requests(cancel_workers=True)
        self._board.cleanup_on_unmount()

    async def on_screen_suspend(self) -> None:
        self._board.stop_background_sync()

    async def _on_task_changed(self, _task_id: str) -> None:
        if not self.is_mounted:
            return
        self._board.schedule_refresh()

    def on_descendant_focus(self, event: events.DescendantFocus) -> None:
        self._board.update_keybinding_hints()
        self.refresh_bindings()

    def on_resize(self, event: events.Resize) -> None:
        self._board.check_screen_size()

    async def on_screen_resume(self) -> None:
        self._board.start_background_sync()
        await self._board.refresh_board()
        self._board.sync_agent_states()

    async def reset_for_repo_change(self) -> None:
        await self._board.reset_for_repo_change()

    async def prepare_for_planner_return(self) -> None:
        """Ensure board reflects planner-created tasks when returning from planner."""
        self._clear_search_state(cancel_workers=True)
        await self._board.refresh_board()
        self._board.sync_agent_states()

    @on(OfflineBanner.Reconnect)
    def on_offline_banner_reconnect(self, event: OfflineBanner.Reconnect) -> None:
        """Handle reconnect from offline banner - refresh agent health check."""
        self.ctx.api.refresh_agent_health()
        if self.ctx.api.is_agent_available():
            self._agent_offline = False

            for banner in self.query(OfflineBanner):
                banner.remove()
            self.notify("Agent is now available", severity="information")
        else:
            self.notify("Agent still unavailable", severity="warning")

    def action_focus_left(self) -> None:
        self.focus_horizontal(-1)

    def action_focus_right(self) -> None:
        self.focus_horizontal(1)

    def action_focus_up(self) -> None:
        self.focus_vertical(-1)

    def action_focus_down(self) -> None:
        self.focus_vertical(1)

    def action_deselect(self) -> None:
        try:
            overlay = self.query_one("#peek-overlay", PeekOverlay)
            if overlay.has_class("visible"):
                overlay.hide()
                return
        except NoMatches:
            pass
        if self.search_visible:
            self._clear_search_state(cancel_workers=True)
            self.run_worker(self._board.refresh_board())
            return
        self.app.set_focus(None)

    def action_quit(self) -> None:
        self.app.exit()

    def action_interrupt(self) -> None:
        self.app.exit()

    def action_toggle_peek(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.TOGGLE_PEEK)

    async def _toggle_peek_flow(self, task: Task) -> None:
        card = self.get_focused_card()
        if not card or not card.task_model:
            return
        try:
            overlay = self.query_one("#peek-overlay", PeekOverlay)
        except NoMatches:
            return
        if not overlay.toggle():
            return

        if task.task_type == TaskType.AUTO:
            if self._is_runtime_running(task.id):
                run_count = self._runtime_run_count(task.id)
                run_label = f" (Run {run_count})" if run_count > 0 else ""
                status = f"🟢 Running{run_label}"
            else:
                status = "⚪ Idle"
        else:
            is_active = await self.ctx.api.session_exists(task.id)
            status = "🟢 Session Active" if is_active else "⚪ No Active Session"

        scratchpad = await self.ctx.api.get_scratchpad(task.id)
        content = scratchpad if scratchpad else "(No scratchpad)"

        overlay.update_content(task.short_id, task.title, status, content)
        x_pos = min(card.region.x + card.region.width + 2, self.size.width - 55)
        y_pos = max(1, card.region.y)
        overlay.show_at(x_pos, y_pos)

    def on_key(self, event: events.Key) -> None:
        has_focused_card = self.get_focused_card() is not None
        if event.key == "enter" and has_focused_card and not self.search_visible:
            if self._dispatch_kanban_action(KanbanActionId.OPEN_SESSION):
                event.stop()
                return

        key_action_map = {
            binding.key: action_id
            for binding in KANBAN_BINDINGS
            if isinstance(binding, Binding)
            and (action_id := _as_kanban_action_id(binding.action))
            in KANBAN_ACTIONS_WITH_KEY_FEEDBACK
        }
        action_id = key_action_map.get(event.key)
        if action_id is None:
            return
        _, reason = self._validate_action(action_id)
        if reason:
            self.notify(reason, severity="warning")

    def action_toggle_search(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.TOGGLE_SEARCH)

    async def _toggle_search_flow(self) -> None:
        if self.search_visible:
            self._clear_search_state(cancel_workers=True)
            await self._board.refresh_board()
            return
        self.search_visible = True

    @on(SearchBar.QueryChanged)
    def on_search_query_changed(self, event: SearchBar.QueryChanged) -> None:
        if not self.search_visible:
            return
        request_id = self._next_search_request_id()
        query = event.query.strip()
        self.run_worker(
            self._apply_search_query(query, request_id),
            group="kanban-search",
            exclusive=True,
            exit_on_error=False,
        )

    async def _apply_search_query(self, query: str, request_id: int) -> None:
        try:
            filtered_tasks = None if not query else await self.ctx.api.search_tasks(query)
        except asyncio.CancelledError:
            return
        if request_id != self._search_request_id or not self.is_mounted or not self.search_visible:
            return
        self._ui_state.filtered_tasks = filtered_tasks
        await self._board.refresh_board()

    def _next_search_request_id(self) -> int:
        self._search_request_id += 1
        return self._search_request_id

    def invalidate_search_requests(self, *, cancel_workers: bool = False) -> None:
        self._search_request_id += 1
        if not cancel_workers:
            return
        for worker in self.workers:
            if worker.node == self and worker.group == "kanban-search" and not worker.is_finished:
                worker.cancel()

    def _clear_search_state(self, *, cancel_workers: bool) -> None:
        self.invalidate_search_requests(cancel_workers=cancel_workers)
        self.search_visible = False
        self._ui_state.filtered_tasks = None

    def _focused_task(self, *, notify_on_missing: bool = False) -> Task | None:
        card = self.get_focused_card()
        if not card or not card.task_model:
            if notify_on_missing:
                self.notify("No task selected", severity="warning")
            return None
        return card.task_model

    def _run_worker_for_action(
        self, action: str | KanbanActionId, operation: Awaitable[None]
    ) -> None:
        spec = get_kanban_action(action)
        if spec is None:
            self.run_worker(operation)
            return

        if spec.worker_group is not None:
            self.run_worker(
                operation,
                group=spec.worker_group,
                exclusive=spec.exclusive,
                exit_on_error=spec.exit_on_error,
            )
            return

        self.run_worker(
            operation,
            exclusive=spec.exclusive,
            exit_on_error=spec.exit_on_error,
        )

    def _dispatch_kanban_action(
        self,
        action: str | KanbanActionId,
        *,
        notify_on_missing_task: bool = False,
    ) -> bool:
        operation = self._resolve_kanban_action_operation(
            action,
            notify_on_missing_task=notify_on_missing_task,
        )
        if operation is None:
            return False
        self._run_worker_for_action(action, operation)
        return True

    def run_kanban_action(self, action: str) -> None:
        """Dispatch a Kanban action through the shared action table."""
        notify_on_missing = action == KanbanActionId.DUPLICATE_TASK
        if self._dispatch_kanban_action(action, notify_on_missing_task=notify_on_missing):
            return
        action_method = getattr(self, f"action_{action}", None)
        if action_method is None:
            return
        result = action_method()
        if asyncio.iscoroutine(result):
            self.run_worker(result)

    def _resolve_kanban_action_operation(
        self,
        action: str | KanbanActionId,
        *,
        notify_on_missing_task: bool = False,
    ) -> Awaitable[None] | None:
        action_id = _as_kanban_action_id(action)
        action_value = action_id if action_id is not None else str(action)
        spec = get_kanban_action(action_value)
        task: Task | None = None
        if spec is not None and spec.requires_task:
            task = self._focused_task(notify_on_missing=notify_on_missing_task)
            if task is None:
                return None

        if task is not None:
            task_operation = self._resolve_task_action_operation(action_id, task)
            if task_operation is not None:
                return task_operation
        return self._resolve_global_action_operation(action_id)

    def _resolve_task_action_operation(
        self, action: KanbanActionId | None, task: Task
    ) -> Awaitable[None] | None:
        if action is None:
            return None
        task_operations: dict[KanbanActionId, Callable[[Task], Awaitable[None]]] = {
            KanbanActionId.EDIT_TASK: lambda selected: self._open_task_details_modal(
                task=selected,
                start_editing=True,
            ),
            KanbanActionId.DELETE_TASK_DIRECT: self._confirm_and_delete_task,
            KanbanActionId.DUPLICATE_TASK: self._run_duplicate_task_flow,
            KanbanActionId.VIEW_DETAILS: lambda selected: self._open_task_details_modal(
                task=selected
            ),
            KanbanActionId.TOGGLE_PEEK: self._toggle_peek_flow,
            KanbanActionId.OPEN_SESSION: self._session.open_session_flow,
            KanbanActionId.START_AGENT: self._session.start_agent_flow,
            KanbanActionId.STOP_AGENT: self._stop_agent_flow,
            KanbanActionId.SET_TASK_BRANCH: self._set_task_branch_flow,
            KanbanActionId.MERGE_DIRECT: lambda _selected: self._review.action_merge_direct(),
            KanbanActionId.REBASE: lambda _selected: self._review.action_rebase(),
            KanbanActionId.MOVE_FORWARD: lambda _selected: self._review.move_task(forward=True),
            KanbanActionId.MOVE_BACKWARD: lambda _selected: self._review.move_task(forward=False),
            KanbanActionId.VIEW_DIFF: lambda _selected: self._review.action_view_diff(),
            KanbanActionId.OPEN_REVIEW: lambda _selected: self._review.action_open_review(),
        }
        operation_factory = task_operations.get(action)
        if operation_factory is None:
            return None
        return operation_factory(task)

    def _resolve_global_action_operation(
        self, action: KanbanActionId | None
    ) -> Awaitable[None] | None:
        if action is None:
            return None
        global_operations: dict[KanbanActionId, Callable[[], Awaitable[None]]] = {
            KanbanActionId.NEW_TASK: self._open_task_details_modal,
            KanbanActionId.NEW_AUTO_TASK: lambda: self._open_task_details_modal(
                initial_type=TaskType.AUTO
            ),
            KanbanActionId.TOGGLE_SEARCH: self._toggle_search_flow,
            KanbanActionId.SWITCH_GLOBAL_AGENT: self._switch_global_agent_flow,
            KanbanActionId.OPEN_SETTINGS: self._open_settings_flow,
            KanbanActionId.SET_DEFAULT_BRANCH: self._set_default_branch_flow,
            KanbanActionId.OPEN_PLANNER: self._open_planner_flow,
        }
        operation_factory = global_operations.get(action)
        if operation_factory is None:
            return None
        return operation_factory()

    def action_new_task(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.NEW_TASK)

    def action_new_auto_task(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.NEW_AUTO_TASK)

    async def _open_task_details_modal(
        self,
        *,
        task: Task | None = None,
        start_editing: bool = False,
        initial_type: TaskType | None = None,
    ) -> None:
        await self._task_controller.open_task_details_modal(
            task=task,
            start_editing=start_editing,
            initial_type=initial_type,
        )

    async def _handle_task_details_result(
        self,
        result: object | dict | None,
        *,
        editing_task_id: str | None,
    ) -> None:
        await self._task_controller.handle_task_details_result(
            result,
            editing_task_id=editing_task_id,
        )

    async def _save_task_modal_changes(
        self,
        result: dict,
        *,
        editing_task_id: str | None,
    ) -> None:
        await self._task_controller.save_task_modal_changes(result, editing_task_id=editing_task_id)

    async def _create_task_from_payload(self, payload: dict) -> Task:
        return await self._task_controller.create_task_from_payload(payload)

    def _extract_non_content_update_fields(self, payload: dict) -> dict:
        return self._task_controller.extract_non_content_update_fields(payload)

    async def _handle_task_type_transition(
        self, task: Task, update_fields: dict[str, object]
    ) -> bool:
        return await self._task_controller.handle_task_type_transition(task, update_fields)

    def action_edit_task(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.EDIT_TASK)

    def action_delete_task_direct(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.DELETE_TASK_DIRECT)

    async def _confirm_and_delete_task(self, task: Task) -> None:
        await self._task_controller.confirm_and_delete_task(task)

    def action_merge_direct(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.MERGE_DIRECT)

    def action_rebase(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.REBASE)

    def action_move_forward(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.MOVE_FORWARD)

    def action_move_backward(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.MOVE_BACKWARD)

    def action_duplicate_task(self) -> None:
        self._dispatch_kanban_action(
            KanbanActionId.DUPLICATE_TASK,
            notify_on_missing_task=True,
        )

    async def _run_duplicate_task_flow(self, source_task: Task) -> None:
        await self._task_controller.run_duplicate_task_flow(source_task)

    def action_copy_task_id(self) -> None:
        task = self._focused_task(notify_on_missing=True)
        if task is None:
            return
        copy_with_notification(self.app, f"#{task.short_id}", "Task ID")

    def action_view_details(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.VIEW_DETAILS)

    def action_expand_description(self) -> None:
        """Expand description in full-screen editor (read-only from Kanban)."""
        card = self.get_focused_card()
        if not card or not card.task_model:
            self.notify("No task selected", severity="warning")
            return
        description = card.task_model.description or ""
        modal = DescriptionEditorModal(
            description=description, readonly=True, title="View Description"
        )
        self.app.push_screen(modal)

    def action_open_session(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.OPEN_SESSION)

    def action_start_agent(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.START_AGENT)

    async def action_stop_agent(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.STOP_AGENT)

    async def _stop_agent_flow(self, task: Task) -> None:
        await self._task_controller.stop_agent_flow(task)

    def action_open_planner(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.OPEN_PLANNER)

    async def _open_planner_flow(self) -> None:
        self.app.push_screen(PlannerScreen(agent_factory=self.kagan_app._agent_factory))

    def action_switch_global_agent(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.SWITCH_GLOBAL_AGENT)

    async def _switch_global_agent_flow(self) -> None:
        from kagan.tui.ui.modals import GlobalAgentPickerModal

        current_agent = self.kagan_app.config.general.default_worker_agent
        selected = await await_screen_result(
            self.app, GlobalAgentPickerModal(current_agent=current_agent)
        )
        if not selected:
            return
        await self._session.apply_global_agent_selection(selected)

    async def action_open_settings(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.OPEN_SETTINGS)

    async def _open_settings_flow(self) -> None:
        from kagan.tui.ui.modals import SettingsModal

        config = self.kagan_app.config
        config_path = self.kagan_app.config_path
        result = await await_screen_result(
            self.app,
            SettingsModal(config, config_path),
        )
        self._on_settings_result(result)

    def _on_settings_result(self, result: bool | None) -> None:
        if not result:
            return
        config_path = self.kagan_app.config_path
        self.kagan_app.config = self.kagan_app.config.load(config_path)
        self.header.update_agent_from_config(self.kagan_app.config)
        self.notify("Settings saved")

    def action_view_diff(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.VIEW_DIFF)

    def action_open_review(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.OPEN_REVIEW)

    def action_set_task_branch(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.SET_TASK_BRANCH)

    async def _set_task_branch_flow(self, task: Task) -> None:
        await self._task_controller.set_task_branch_flow(task)

    async def _update_task_branch(self, task: Task, branch: str) -> None:
        await self._task_controller.update_task_branch(task, branch)

    def action_set_default_branch(self) -> None:
        self._dispatch_kanban_action(KanbanActionId.SET_DEFAULT_BRANCH)

    async def _set_default_branch_flow(self) -> None:
        await self._task_controller.set_default_branch_flow()

    def on_task_card_selected(self, message: TaskCard.Selected) -> None:
        self.action_view_details()
