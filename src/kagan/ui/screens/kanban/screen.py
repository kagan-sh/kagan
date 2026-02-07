"""Main Kanban board screen."""

from __future__ import annotations

import asyncio
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
)
from kagan.core.models.enums import CardIndicator, TaskStatus, TaskType
from kagan.git_utils import list_local_branches
from kagan.keybindings import KANBAN_BINDINGS
from kagan.ui.modals import (
    BaseBranchModal,
    ConfirmModal,
    ModalAction,
    TaskDetailsModal,
)
from kagan.ui.modals.description_editor import DescriptionEditorModal
from kagan.ui.screens.base import KaganScreen
from kagan.ui.screens.kanban import focus
from kagan.ui.screens.kanban.board_controller import KanbanBoardController
from kagan.ui.screens.kanban.review_controller import KanbanReviewController
from kagan.ui.screens.kanban.session_controller import KanbanSessionController
from kagan.ui.screens.planner import PlannerScreen
from kagan.ui.utils import copy_with_notification
from kagan.ui.widgets.column import KanbanColumn
from kagan.ui.widgets.header import KaganHeader
from kagan.ui.widgets.keybinding_hint import KanbanHintBar
from kagan.ui.widgets.offline_banner import OfflineBanner
from kagan.ui.widgets.peek_overlay import PeekOverlay
from kagan.ui.widgets.search_bar import SearchBar

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path

    from textual import events
    from textual.app import ComposeResult
    from textual.timer import Timer

    from kagan.config import AgentConfig
    from kagan.core.models.entities import Task
    from kagan.mcp.global_config import GlobalMcpSpec
    from kagan.ui.widgets.card import TaskCard

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
        self._refresh_timer: Timer | None = None
        self._task_hashes: dict[str, int] = {}
        self._agent_offline: bool = False
        self._merge_failed_tasks: set[str] = set()
        self._board = KanbanBoardController(self)
        self._review = KanbanReviewController(self)
        self._session = KanbanSessionController(self)

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
        self._board.check_agent_health()

    def on_unmount(self) -> None:
        self._board.cleanup_on_unmount()

    async def _on_task_changed(self, _task_id: str) -> None:
        if not self.is_mounted:
            return
        self._schedule_refresh()

    def _sync_agent_states(self) -> None:
        self._board.sync_agent_states()

    def _set_card_indicator(
        self,
        task_id: str,
        indicator,
        *,
        is_active: bool | None = None,
    ) -> None:
        self._board.set_card_indicator(task_id, indicator, is_active=is_active)

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
        await self._board.reset_for_repo_change()

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
        self._board.check_screen_size()

    async def _refresh_board(self) -> None:
        await self._board.refresh_board()

    def _notify_status_changes(
        self,
        new_tasks: list[Task],
        old_status_by_id: dict[str, TaskStatus],
        changed_ids: set[str],
    ) -> None:
        self._board.notify_status_changes(new_tasks, old_status_by_id, changed_ids)

    def _restore_focus(self, task_id: str) -> None:
        self._board.restore_focus(task_id)

    async def _refresh_and_sync(self) -> None:
        await self._board.refresh_and_sync()

    def _schedule_refresh(self) -> None:
        self._board.schedule_refresh()

    def _run_refresh(self) -> None:
        self._board.run_refresh()

    def _update_review_queue_hint(self) -> None:
        self._board.update_review_queue_hint()

    def _update_keybinding_hints(self) -> None:
        self._board.update_keybinding_hints()

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
        await self._review.action_merge_direct()

    async def action_rebase(self) -> None:
        await self._review.action_rebase()

    async def _handle_rebase_conflict(
        self, task: Task, base_branch: str, conflict_files: list[str]
    ) -> None:
        await self._review.handle_rebase_conflict(task, base_branch, conflict_files)

    async def _move_task(self, forward: bool) -> None:
        await self._review.move_task(forward)

    async def _on_merge_confirmed(self, confirmed: bool | None) -> None:
        await self._review.on_merge_confirmed(confirmed)

    async def _on_close_confirmed(self, confirmed: bool | None) -> None:
        await self._review.on_close_confirmed(confirmed)

    async def _on_advance_confirmed(self, confirmed: bool | None) -> None:
        await self._review.on_advance_confirmed(confirmed)

    async def _on_auto_move_confirmed(self, confirmed: bool | None) -> None:
        await self._review.on_auto_move_confirmed(confirmed)

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
        return await self._session.provision_workspace_for_active_repo(task)

    async def _ensure_mcp_installed(self, agent_config: AgentConfig, spec: GlobalMcpSpec) -> bool:
        return await self._session.ensure_mcp_installed(agent_config, spec)

    async def _handle_missing_agent(self, task: Task, agent_config: AgentConfig) -> str:
        return await self._session.handle_missing_agent(task, agent_config)

    async def _update_task_agent(self, task: Task, agent_short_name: str) -> None:
        await self._session.update_task_agent(task, agent_short_name)

    async def _ask_confirmation(self, title: str, message: str) -> bool:
        return await self._session.ask_confirmation(title, message)

    async def _confirm_start_auto_task(self, task: Task) -> bool:
        return await self._session.confirm_start_auto_task(task)

    async def _confirm_attach_pair_session(self, task: Task) -> bool:
        return await self._session.confirm_attach_pair_session(task)

    async def _open_auto_output_for_task(
        self,
        task: Task,
        *,
        wait_for_running: bool = False,
    ) -> None:
        await self._session.open_auto_output_for_task(task, wait_for_running=wait_for_running)

    async def _open_session_flow(self, task: Task) -> None:
        await self._session.open_session_flow(task)

    def _resolve_pair_terminal_backend(self, task: Task) -> str:
        return self._session.resolve_pair_terminal_backend(task)

    async def _ensure_pair_terminal_backend_ready(self, task: Task) -> bool:
        return await self._session.ensure_pair_terminal_backend_ready(task)

    @staticmethod
    def _startup_prompt_path_hint(workspace_path: Path) -> Path:
        return KanbanSessionController.startup_prompt_path_hint(workspace_path)

    async def _do_open_pair_session(
        self,
        task: Task,
        workspace_path: Path | None = None,
        terminal_backend: str | None = None,
    ) -> None:
        await self._session.do_open_pair_session(
            task,
            workspace_path=workspace_path,
            terminal_backend=terminal_backend,
        )

    def action_start_agent(self) -> None:
        card = focus.get_focused_card(self)
        if not card or not card.task_model:
            return
        self.run_worker(
            self._start_agent_flow(card.task_model),
            group=f"start-agent-{card.task_model.id}",
        )

    async def _start_agent_flow(self, task: Task) -> None:
        await self._session.start_agent_flow(task)

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
        await self._session.apply_global_agent_selection(selected)

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
        return self._review.get_review_task(card)

    async def action_merge(self) -> None:
        await self._review.action_merge()

    async def action_view_diff(self) -> None:
        await self._review.action_view_diff()

    async def _on_diff_result(self, task: Task, result: str | None) -> None:
        await self._review.on_diff_result(task, result)

    async def action_open_review(self) -> None:
        await self._review.action_open_review()

    async def _open_review_for_task(
        self,
        task: Task,
        *,
        read_only: bool = False,
        initial_tab: str | None = None,
        include_running_output: bool = False,
    ) -> None:
        await self._review.open_review_for_task(
            task,
            read_only=read_only,
            initial_tab=initial_tab,
            include_running_output=include_running_output,
        )

    async def _on_review_result(self, task_id: str, result: str | None) -> None:
        await self._review.on_review_result(task_id, result)

    async def _execute_merge(
        self,
        task: Task,
        *,
        success_msg: str,
        track_failures: bool = False,
    ) -> bool:
        return await self._review.execute_merge(
            task,
            success_msg=success_msg,
            track_failures=track_failures,
        )

    @staticmethod
    def _format_merge_failure(task: Task, message: str) -> str:
        return KanbanReviewController.format_merge_failure(task, message)

    def _confirm_close_no_changes(self, task: Task) -> None:
        self._review.confirm_close_no_changes(task)

    async def _handle_reject_with_feedback(self, task: Task) -> None:
        await self._review.handle_reject_with_feedback(task)

    async def _apply_rejection_result(self, task: Task, result: tuple[str, str] | None) -> None:
        await self._review.apply_rejection_result(task, result)

    async def _save_pair_instructions_preference(self, skip: bool = True) -> None:
        await self._session.save_pair_instructions_preference(skip=skip)

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
