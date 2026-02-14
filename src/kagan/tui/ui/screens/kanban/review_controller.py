from __future__ import annotations

from typing import TYPE_CHECKING, Any

from kagan.core.constants import NOTIFICATION_TITLE_MAX_LENGTH
from kagan.core.models.enums import (
    CardIndicator,
    RejectionAction,
    ReviewResult,
    TaskStatus,
    TaskType,
)
from kagan.core.services.jobs import JobStatus
from kagan.core.time import utc_now
from kagan.tui.ui.screen_result import await_screen_result

if TYPE_CHECKING:
    from kagan.core.adapters.db.schema import Task
    from kagan.core.services.runtime import AutoOutputReadiness
    from kagan.tui.ui.screens.kanban.screen import KanbanScreen
    from kagan.tui.ui.widgets.card import TaskCard


class KanbanReviewController:
    def __init__(self, screen: KanbanScreen) -> None:
        self.screen = screen

    @staticmethod
    def _state_attr(state: object | None, name: str, default: Any = None) -> Any:
        if state is None:
            return default
        if isinstance(state, dict):
            return state.get(name, default)
        return getattr(state, name, default)

    @classmethod
    def _state_bool(cls, state: object | None, name: str) -> bool:
        return bool(cls._state_attr(state, name, False))

    @classmethod
    def _state_tuple(cls, state: object | None, name: str) -> tuple[str, ...]:
        value = cls._state_attr(state, name, ())
        if isinstance(value, (list, tuple)):
            return tuple(str(item) for item in value if str(item).strip())
        return ()

    @classmethod
    def _state_int(cls, state: object | None, name: str, default: int = 0) -> int:
        value = cls._state_attr(state, name, default)
        try:
            return int(value)
        except (TypeError, ValueError):
            return default

    @classmethod
    def _stream_target(cls, state: object | None, name: str) -> object | None:
        candidate = cls._state_attr(state, name)
        if callable(getattr(candidate, "set_message_target", None)):
            return candidate
        return None

    async def _resolve_base_branch(self, task: Task) -> str | None:
        try:
            return await self.screen.ctx.api.resolve_task_base_branch(task)
        except ValueError as exc:
            self.screen.notify(str(exc), severity="error")
            return None

    def get_review_task(self, card: TaskCard | None) -> Task | None:
        if not card or not card.task_model:
            return None
        if card.task_model.status != TaskStatus.REVIEW:
            self.screen.notify("Task is not in REVIEW", severity="warning")
            return None
        return card.task_model

    async def action_merge_direct(self) -> None:
        task = self.get_review_task(self.screen.get_focused_card())
        if not task:
            return
        if await self.screen.ctx.api.has_no_changes(task):
            await self.confirm_close_no_changes(task)
            return
        await self.execute_merge(
            task,
            success_msg=f"Merged: {task.title}",
            track_failures=True,
        )

    async def action_rebase(self) -> None:
        task = self.get_review_task(self.screen.get_focused_card())
        if not task:
            return
        base = await self._resolve_base_branch(task)
        if base is None:
            return
        self.screen.notify("Rebasing... (this may take a few seconds)", severity="information")
        (
            success,
            message,
            conflict_files,
        ) = await self.screen.ctx.api.rebase_workspace(task.id, base)
        if success:
            self.screen.notify(f"Rebased: {task.title}", severity="information")
        elif conflict_files:
            await self.handle_rebase_conflict(task, base, conflict_files)
        else:
            self.screen.notify(f"Rebase failed: {message}", severity="error")

    async def handle_rebase_conflict(
        self, task: Task, base_branch: str, conflict_files: list[str]
    ) -> None:
        from kagan.core.agents.prompt_builders import build_conflict_resolution_instructions

        await self.screen.ctx.api.abort_workspace_rebase(task.id)

        workspaces = await self.screen.ctx.api.list_workspaces(task_id=task.id)
        branch_name = workspaces[0].branch_name if workspaces else f"task-{task.short_id}"
        instructions = build_conflict_resolution_instructions(
            source_branch=branch_name,
            target_branch=base_branch,
            conflict_files=conflict_files,
        )

        timestamp = utc_now().strftime("%Y-%m-%d %H:%M")
        separator = f"\n\n---\n_Rebase conflict detected ({timestamp}):_\n\n"
        current_desc = task.description or ""
        new_desc = current_desc + separator + instructions
        await self.screen.ctx.api.update_task(task.id, description=new_desc)

        await self.screen.ctx.api.move_task(task.id, TaskStatus.IN_PROGRESS)

        if task.task_type == TaskType.AUTO:
            refreshed = await self.screen.ctx.api.get_task(task.id)
            if refreshed:
                submitted = await self.screen.ctx.api.submit_job(
                    refreshed.id,
                    "start_agent",
                )
                terminal = await self.screen._session.wait_for_job_terminal(
                    submitted.job_id,
                    task_id=refreshed.id,
                )
                payload = self.screen._session.job_result_payload(terminal)
                failed = payload is not None and not bool(payload.get("success", False))
                cancelled_or_failed = terminal is not None and terminal.status in {
                    JobStatus.FAILED,
                    JobStatus.CANCELLED,
                }
                if failed or cancelled_or_failed:
                    self.screen.notify(
                        self.screen._session.job_message(
                            terminal, "Failed to restart AUTO agent after conflict"
                        ),
                        severity="warning",
                    )
                elif payload is None:
                    self.screen.notify(
                        self.screen._session.START_JOB_PENDING_MESSAGE,
                        severity="information",
                    )

        self.screen._merge_failed_tasks.add(task.id)

        await self.screen._board.refresh_and_sync()
        n_files = len(conflict_files)
        self.screen.notify(
            f"Rebase conflict: {n_files} file(s). Task moved to IN_PROGRESS.",
            severity="warning",
        )

    async def move_task(self, forward: bool) -> None:
        card = self.screen.get_focused_card()
        if not card or not card.task_model:
            return
        task = card.task_model
        status = task.status
        task_type = task.task_type

        new_status = TaskStatus.next_status(status) if forward else TaskStatus.prev_status(status)
        if new_status:
            if status == TaskStatus.IN_PROGRESS and task_type == TaskType.AUTO:
                await self.on_auto_move_confirmed(task, new_status)
                return

            if status == TaskStatus.REVIEW and new_status == TaskStatus.DONE:
                if await self.screen.ctx.api.has_no_changes(task):
                    await self.confirm_close_no_changes(task)
                    return
                await self.on_merge_confirmed(task)
                return

            if (
                status == TaskStatus.IN_PROGRESS
                and task_type == TaskType.PAIR
                and new_status == TaskStatus.REVIEW
            ):
                await self.on_advance_confirmed(task)
                return

            if (
                task_type == TaskType.AUTO
                and status == TaskStatus.IN_PROGRESS
                and new_status != TaskStatus.REVIEW
            ):
                self.screen._board.set_card_indicator(task.id, CardIndicator.IDLE, is_active=False)

            await self.screen.ctx.api.move_task(task.id, new_status)
            await self.screen._board.refresh_board()
            self.screen.notify(f"Moved #{task.id} to {new_status.value}")
            self.screen.focus_column(new_status)
        else:
            self.screen.notify(
                f"Already in {'final' if forward else 'first'} status",
                severity="warning",
            )

    async def on_merge_confirmed(self, task: Task) -> None:
        from kagan.tui.ui.modals import ConfirmModal

        self.screen._ui_state.pending_merge_task = task
        title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
        confirmed = await await_screen_result(
            self.screen.app,
            ConfirmModal(
                title="Complete Task?",
                message=f"Merge '{title}' and move to DONE?",
            ),
        )
        pending_task = self.screen._ui_state.pending_merge_task
        self.screen._ui_state.pending_merge_task = None
        if not confirmed or pending_task is None:
            return
        await self.execute_merge(
            pending_task,
            success_msg=f"Merged and completed: {pending_task.title}",
            track_failures=True,
        )

    async def on_close_confirmed(self, task: Task) -> None:
        from kagan.tui.ui.modals import ConfirmModal

        self.screen._ui_state.pending_close_task = task
        title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
        confirmed = await await_screen_result(
            self.screen.app,
            ConfirmModal(
                title="No Changes Detected",
                message=f"Mark '{title}' as DONE and archive the workspace?",
            ),
        )
        pending_task = self.screen._ui_state.pending_close_task
        self.screen._ui_state.pending_close_task = None
        if not confirmed or pending_task is None:
            return
        success, message = await self.screen.ctx.api.close_exploratory(pending_task)
        if success:
            await self.screen._board.refresh_board()
            self.screen.notify(f"Closed (no changes): {pending_task.title}")
            return
        self.screen.notify(message, severity="error")

    async def on_advance_confirmed(self, task: Task) -> None:
        from kagan.tui.ui.modals import ConfirmModal

        self.screen._ui_state.pending_advance_task = task
        title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
        confirmed = await await_screen_result(
            self.screen.app,
            ConfirmModal(title="Advance to Review?", message=f"Move '{title}' to REVIEW?"),
        )
        pending_task = self.screen._ui_state.pending_advance_task
        self.screen._ui_state.pending_advance_task = None
        if not confirmed or pending_task is None:
            return
        await self.screen.ctx.api.update_task(pending_task.id, status=TaskStatus.REVIEW)
        await self.screen._board.refresh_board()
        self.screen.notify(f"Moved #{pending_task.id} to REVIEW")
        self.screen.focus_column(TaskStatus.REVIEW)

    async def on_auto_move_confirmed(self, task: Task, new_status: TaskStatus) -> None:
        """Handle auto move confirmed event."""
        from kagan.tui.ui.modals import ConfirmModal

        self.screen._ui_state.pending_auto_move_task = task
        self.screen._ui_state.pending_auto_move_status = new_status
        title = task.title[:NOTIFICATION_TITLE_MAX_LENGTH]
        destination = new_status.value.upper()
        confirmed = await await_screen_result(
            self.screen.app,
            ConfirmModal(
                title="Stop Agent and Move Task?",
                message=f"Stop agent, keep worktree/logs, and move '{title}' to {destination}?",
            ),
        )
        pending_task = self.screen._ui_state.pending_auto_move_task
        pending_status = self.screen._ui_state.pending_auto_move_status
        self.screen._ui_state.pending_auto_move_task = None
        self.screen._ui_state.pending_auto_move_status = None
        if not confirmed or pending_task is None or pending_status is None:
            return

        submitted = await self.screen.ctx.api.submit_job(
            pending_task.id,
            "stop_agent",
        )
        terminal = await self.screen._session.wait_for_job_terminal(
            submitted.job_id,
            task_id=pending_task.id,
        )
        payload = self.screen._session.job_result_payload(terminal)
        if payload is None:
            self.screen.notify(
                self.screen._session.STOP_JOB_PENDING_MESSAGE,
                severity="information",
            )
            return
        elif terminal is not None and terminal.status in {JobStatus.FAILED, JobStatus.CANCELLED}:
            self.screen.notify(
                self.screen._session.job_message(terminal, "No running agent for this task"),
                severity="warning",
            )
            return
        runtime = payload.get("runtime") if isinstance(payload, dict) else None
        runtime_running = isinstance(runtime, dict) and bool(runtime.get("is_running", False))
        if runtime_running or self.screen.ctx.api.is_automation_running(pending_task.id):
            self.screen.notify(self.screen._session.job_message(terminal, "Agent stop queued"))
            return
        was_running = bool(payload.get("success", False)) if payload is not None else False
        self.screen._board.set_card_indicator(pending_task.id, CardIndicator.IDLE, is_active=False)

        await self.screen.ctx.api.move_task(pending_task.id, pending_status)
        await self.screen._board.refresh_board()
        if was_running:
            self.screen.notify(
                f"Moved #{pending_task.id} to {pending_status.value} (agent stopped)"
            )
        else:
            self.screen.notify(f"Moved #{pending_task.id} to {pending_status.value}")
        self.screen.focus_column(pending_status)

    async def action_merge(self) -> None:
        task = self.get_review_task(self.screen.get_focused_card())
        if not task:
            return
        if await self.screen.ctx.api.has_no_changes(task):
            await self.confirm_close_no_changes(task)
            return
        await self.execute_merge(task, success_msg=f"Merged and completed: {task.title}")

    async def action_view_diff(self) -> None:
        from kagan.tui.ui.modals import DiffModal

        task = self.get_review_task(self.screen.get_focused_card())
        if not task:
            return
        title = f"Diff: {task.short_id} {task.title[:NOTIFICATION_TITLE_MAX_LENGTH]}"
        workspaces = await self.screen.ctx.api.list_workspaces(task_id=task.id)

        if not workspaces:
            base = await self._resolve_base_branch(task)
            if base is None:
                return
            diff_text = await self.screen.ctx.api.get_workspace_diff(task.id, base_branch=base)
            result = await await_screen_result(
                self.screen.app, DiffModal(title=title, diff_text=diff_text, task=task)
            )
            await self.on_diff_result(task, result)
            return

        try:
            diffs = await self.screen.ctx.api.get_all_diffs(workspaces[0].id)
        except RuntimeError:
            base = await self._resolve_base_branch(task)
            if base is None:
                return
            diff_text = await self.screen.ctx.api.get_workspace_diff(task.id, base_branch=base)
            result = await await_screen_result(
                self.screen.app, DiffModal(title=title, diff_text=diff_text, task=task)
            )
            await self.on_diff_result(task, result)
            return

        result = await await_screen_result(
            self.screen.app,
            DiffModal(title=title, diffs=diffs, task=task),
        )
        await self.on_diff_result(task, result)

    async def on_diff_result(self, task: Task, result: str | None) -> None:
        if result == ReviewResult.APPROVE:
            if await self.screen.ctx.api.has_no_changes(task):
                await self.confirm_close_no_changes(task)
                return
            await self.execute_merge(task, success_msg=f"Merged: {task.title}")
        elif result == ReviewResult.REJECT:
            await self.handle_reject_with_feedback(task)

    async def action_open_review(self) -> None:
        task = self.get_review_task(self.screen.get_focused_card())
        if not task:
            return
        await self.open_task_output_for_task(task)

    async def open_task_output_for_task(
        self,
        task: Task,
        *,
        read_only: bool = False,
        initial_tab: str | None = None,
        include_running_output: bool = False,
        auto_output_readiness: AutoOutputReadiness | None = None,
    ) -> None:
        """Open task output for task."""
        from kagan.tui.ui.modals import ReviewModal

        agent_config = task.get_agent_config(self.screen.kagan_app.config)
        task_id = task.id
        is_auto = task.task_type == TaskType.AUTO
        api = self.screen.ctx.api
        runtime_view = api.get_runtime_view(task.id) if is_auto else None

        if is_auto:
            execution_id = self._state_attr(runtime_view, "execution_id")
            run_count = self._state_int(runtime_view, "run_count")
            is_running = self._state_bool(runtime_view, "is_running")
            is_reviewing = self._state_bool(runtime_view, "is_reviewing")
            is_blocked = self._state_bool(runtime_view, "is_blocked")
            blocked_reason = self._state_attr(runtime_view, "blocked_reason")
            blocked_by_task_ids = self._state_tuple(runtime_view, "blocked_by_task_ids")
            overlap_hints = self._state_tuple(runtime_view, "overlap_hints")
            is_pending = self._state_bool(runtime_view, "is_pending")
            pending_reason = self._state_attr(runtime_view, "pending_reason")
            review_agent = self._stream_target(runtime_view, "review_agent")
        else:
            execution_id = None
            run_count = 0
            is_running = False
            is_reviewing = False
            is_blocked = False
            blocked_reason = None
            blocked_by_task_ids = ()
            overlap_hints = ()
            is_pending = False
            pending_reason = None
            review_agent = None

        running_agent = None
        if is_auto and include_running_output:
            running_agent = self._stream_target(runtime_view, "running_agent")
            if running_agent is not None:
                is_running = True
        if is_auto and auto_output_readiness is not None:
            readiness_execution_id = self._state_attr(auto_output_readiness, "execution_id")
            if readiness_execution_id is not None:
                execution_id = readiness_execution_id
            is_running = self._state_bool(auto_output_readiness, "is_running")
            if include_running_output:
                readiness_running_agent = self._stream_target(
                    auto_output_readiness,
                    "running_agent",
                )
                if readiness_running_agent is not None:
                    running_agent = readiness_running_agent
        if running_agent is not None:
            is_running = True

        if execution_id is None:
            latest = await self.screen.ctx.api.get_latest_execution_for_task(task.id)
            if latest is not None:
                execution_id = latest.id
                if run_count == 0:
                    run_count = await self.screen.ctx.api.count_executions_for_task(task.id)

        resolved_base_branch = await self._resolve_base_branch(task)
        if resolved_base_branch is None:
            return

        result = await await_screen_result(
            self.screen.app,
            ReviewModal(
                task=task,
                agent_config=agent_config,
                base_branch=resolved_base_branch,
                agent_factory=self.screen.kagan_app._agent_factory,
                execution_id=execution_id,
                run_count=run_count,
                running_agent=running_agent,
                review_agent=review_agent,
                is_reviewing=is_reviewing,
                is_running=is_running,
                is_blocked=is_blocked,
                blocked_reason=blocked_reason,
                blocked_by_task_ids=blocked_by_task_ids,
                overlap_hints=overlap_hints,
                is_pending=is_pending,
                pending_reason=pending_reason,
                read_only=read_only,
                initial_tab=initial_tab
                or (
                    "review-ai"
                    if is_auto and task.status == TaskStatus.REVIEW
                    else "review-summary"
                ),
            ),
        )
        await self.on_review_result(task_id, result)

    async def on_review_result(self, task_id: str, result: str | None) -> None:
        task = await self.screen.ctx.api.get_task(task_id)
        if not task:
            return

        if result == "rebase_conflict":
            base = await self._resolve_base_branch(task)
            if base is None:
                return
            (
                _success,
                _msg,
                conflict_files,
            ) = await self.screen.ctx.api.rebase_workspace(task.id, base)
            await self.handle_rebase_conflict(task, base, conflict_files)
            return

        if task.status != TaskStatus.REVIEW:
            return
        if result == ReviewResult.APPROVE:
            if await self.screen.ctx.api.has_no_changes(task):
                await self.confirm_close_no_changes(task)
                return
            await self.execute_merge(
                task,
                success_msg=f"Merged and completed: {task.title}",
                track_failures=True,
            )
        elif result == ReviewResult.EXPLORATORY:
            await self.confirm_close_no_changes(task)
        elif result == ReviewResult.REJECT:
            await self.handle_reject_with_feedback(task)

    async def execute_merge(
        self,
        task: Task,
        *,
        success_msg: str,
        track_failures: bool = False,
    ) -> bool:
        self.screen.notify("Merging... (this may take a few seconds)", severity="information")
        try:
            success, message = await self.screen.ctx.api.merge_task_direct(task)
        except Exception as exc:
            if track_failures:
                self.screen._merge_failed_tasks.add(task.id)
                self.screen._board.sync_agent_states()
            self.screen.notify(f"Merge error: {exc}", severity="error")
            return False
        if success:
            if track_failures:
                self.screen._merge_failed_tasks.discard(task.id)
            await self.screen._board.refresh_board()
            self.screen.notify(success_msg, severity="information")
        else:
            if track_failures:
                self.screen._merge_failed_tasks.add(task.id)
                self.screen._board.sync_agent_states()
            self.screen.notify(self.format_merge_failure(task, message), severity="error")
        return success

    @staticmethod
    def format_merge_failure(task: Task, message: str) -> str:
        """Format merge failure."""
        if task.task_type == TaskType.AUTO:
            return f"Merge failed (AUTO): {message}"
        return f"Merge failed (PAIR): {message}"

    async def confirm_close_no_changes(self, task: Task) -> None:
        await self.on_close_confirmed(task)

    async def handle_reject_with_feedback(self, task: Task) -> None:
        from kagan.tui.ui.modals import RejectionInputModal

        result = await await_screen_result(self.screen.app, RejectionInputModal(task.title))
        await self.apply_rejection_result(task, result)

    async def apply_rejection_result(self, task: Task, result: tuple[str, str] | None) -> None:
        if result is None:
            await self.screen.ctx.api.apply_rejection_feedback(
                task,
                None,
                RejectionAction.BACKLOG,
            )
            action = RejectionAction.BACKLOG
        else:
            feedback, action = result
            await self.screen.ctx.api.apply_rejection_feedback(task, feedback, action)
        await self.screen._board.refresh_board()
        if action == RejectionAction.BACKLOG:
            self.screen.notify(f"Moved to BACKLOG: {task.title}")
        else:
            self.screen.notify(f"Returned to IN_PROGRESS: {task.title}")
