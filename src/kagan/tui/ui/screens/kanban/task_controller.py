from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.core.models.enums import CardIndicator, TaskStatus, TaskType
from kagan.core.services.jobs import JobStatus
from kagan.tui.ui.modals import ConfirmModal, ModalAction, TaskDetailsModal
from kagan.tui.ui.screen_result import await_screen_result
from kagan.tui.ui.screens.branch_candidates import choose_branch_with_modal

if TYPE_CHECKING:
    from kagan.core.adapters.db.schema import Task
    from kagan.tui.ui.screens.kanban.screen import KanbanScreen


BRANCH_LOOKUP_TIMEOUT_SECONDS = 1.0


class KanbanTaskController:
    def __init__(self, screen: KanbanScreen) -> None:
        self.screen = screen

    async def open_task_details_modal(
        self,
        *,
        task: Task | None = None,
        start_editing: bool = False,
        initial_type: TaskType | None = None,
    ) -> None:
        """Open task details modal."""
        editing_task_id = task.id if task is not None else None
        result = await await_screen_result(
            self.screen.app,
            TaskDetailsModal(
                task=task,
                start_editing=start_editing,
                initial_type=initial_type,
            ),
        )
        await self.handle_task_details_result(result, editing_task_id=editing_task_id)

    async def handle_task_details_result(
        self,
        result: object | dict | None,
        *,
        editing_task_id: str | None,
    ) -> None:
        if isinstance(result, dict):
            await self.save_task_modal_changes(result, editing_task_id=editing_task_id)
            return
        if result != ModalAction.DELETE or editing_task_id is None:
            return
        task = await self.screen.ctx.api.get_task(editing_task_id)
        if task is not None:
            await self.confirm_and_delete_task(task)

    async def save_task_modal_changes(
        self,
        result: dict,
        *,
        editing_task_id: str | None,
    ) -> None:
        if editing_task_id is None:
            task = await self.create_task_from_payload(result)
            await self.screen._board.refresh_board()
            self.screen.notify(f"Created task: {task.title}")
            return

        current_task = await self.screen.ctx.api.get_task(editing_task_id)
        if current_task is None:
            self.screen.notify("Task no longer exists", severity="error")
            await self.screen._board.refresh_board()
            return

        update_fields = dict(result)
        if not await self.handle_task_type_transition(current_task, update_fields):
            await self.screen._board.refresh_board()
            return
        await self.screen.ctx.api.update_task(editing_task_id, **update_fields)
        await self.screen._board.refresh_board()
        self.screen.notify("Task updated")

    async def create_task_from_payload(self, payload: dict) -> Task:
        task = await self.screen.ctx.api.create_task(
            str(payload.get("title", "")),
            str(payload.get("description", "")),
        )
        update_fields = self.extract_non_content_update_fields(payload)
        if update_fields:
            await self.screen.ctx.api.update_task(task.id, **update_fields)
        return task

    @staticmethod
    def extract_non_content_update_fields(payload: dict) -> dict:
        return {key: value for key, value in payload.items() if key not in ("title", "description")}

    async def handle_task_type_transition(
        self, task: Task, update_fields: dict[str, object]
    ) -> bool:
        next_type_obj = update_fields.get("task_type")
        if not isinstance(next_type_obj, TaskType):
            return True
        next_type = next_type_obj
        current_type = task.task_type
        if next_type == current_type:
            return True

        if current_type == TaskType.PAIR and next_type == TaskType.AUTO:
            if await self.screen.ctx.api.session_exists(task.id):
                await self.screen.ctx.api.kill_session(task.id)
                self.screen.notify("Closed active PAIR session before switching to AUTO")
            update_fields["terminal_backend"] = None
            return True

        if current_type == TaskType.AUTO and next_type == TaskType.PAIR:
            if self.screen.ctx.api.is_automation_running(task.id):
                submitted = await self.screen.ctx.api.submit_job(
                    task.id,
                    "stop_agent",
                )
                terminal = await self.screen._session.wait_for_job_terminal(
                    submitted.job_id,
                    task_id=task.id,
                )
                payload = self.screen._session.job_result_payload(terminal)
                if payload is None:
                    self.screen.notify(
                        self.screen._session.STOP_JOB_PENDING_MESSAGE,
                        severity="information",
                    )
                    self.screen.notify(
                        "Retry task type change once stop is confirmed.",
                        severity="information",
                    )
                    return False
                if terminal is not None and terminal.status in {
                    JobStatus.FAILED,
                    JobStatus.CANCELLED,
                }:
                    self.screen.notify(
                        self.screen._session.job_message(
                            terminal, "Failed to stop active AUTO run"
                        ),
                        severity="warning",
                    )
                    return False

                runtime = payload.get("runtime") if isinstance(payload, dict) else None
                runtime_running = isinstance(runtime, dict) and bool(
                    runtime.get("is_running", False)
                )
                if runtime_running or self.screen.ctx.api.is_automation_running(task.id):
                    self.screen.notify(
                        self.screen._session.job_message(terminal, "Agent stop queued"),
                        severity="information",
                    )
                    self.screen.notify(
                        "Retry task type change once stop is confirmed.",
                        severity="information",
                    )
                    return False

                if bool(payload.get("success", False)):
                    self.screen.notify("Stopped active AUTO run before switching to PAIR")
                self.screen._board.set_card_indicator(task.id, CardIndicator.IDLE, is_active=False)
        return True

    async def confirm_and_delete_task(self, task: Task) -> None:
        self.screen._ui_state.pending_delete_task = task
        confirmed = await await_screen_result(
            self.screen.app, ConfirmModal(title="Delete Task?", message=f'"{task.title}"')
        )
        pending_task = self.screen._ui_state.pending_delete_task
        self.screen._ui_state.pending_delete_task = None

        if not confirmed or pending_task is None:
            return
        await self.screen.ctx.api.delete_task(pending_task.id)
        await self.screen._board.refresh_board()
        self.screen.notify(f"Deleted task: {pending_task.title}")
        self.screen.focus_first_card()

    async def run_duplicate_task_flow(self, source_task: Task) -> None:
        """Run duplicate task flow."""
        payload: dict[str, object] = {
            "title": source_task.title,
            "description": source_task.description,
            "acceptance_criteria": list(source_task.acceptance_criteria),
            "priority": source_task.priority,
            "task_type": source_task.task_type,
            "agent_backend": source_task.agent_backend,
            "terminal_backend": source_task.terminal_backend,
            "base_branch": source_task.base_branch,
        }
        task = await self.create_task_from_payload(payload)
        await self.screen._board.refresh_board()
        self.screen.notify(f"Created duplicate: #{task.short_id}")
        self.screen.focus_column(TaskStatus.BACKLOG)
        await self.open_task_details_modal(task=task, start_editing=True)

    async def stop_agent_flow(self, task: Task) -> None:
        self.screen.notify("Stopping agent...", severity="information")

        submitted = await self.screen.ctx.api.submit_job(
            task.id,
            "stop_agent",
        )
        terminal = await self.screen._session.wait_for_job_terminal(
            submitted.job_id,
            task_id=task.id,
        )
        payload = self.screen._session.job_result_payload(terminal)
        if payload is not None and not bool(payload.get("success", False)):
            self.screen.notify(
                self.screen._session.job_message(terminal, "No agent running for this task"),
                severity="warning",
            )
            return
        if terminal is not None and terminal.status in {JobStatus.FAILED, JobStatus.CANCELLED}:
            self.screen.notify(
                self.screen._session.job_message(terminal, "No agent running for this task"),
                severity="warning",
            )
            return
        if payload is None:
            self.screen.notify(
                self.screen._session.STOP_JOB_PENDING_MESSAGE,
                severity="information",
            )
            return

        runtime = payload.get("runtime") if isinstance(payload, dict) else None
        runtime_running = isinstance(runtime, dict) and bool(runtime.get("is_running", False))
        if not runtime_running and not self.screen.ctx.api.is_automation_running(task.id):
            self.screen._board.set_card_indicator(task.id, CardIndicator.IDLE, is_active=False)
        self.screen.notify(
            self.screen._session.job_message(terminal, "Agent stop queued"),
            severity="information",
        )

    async def set_task_branch_flow(self, task: Task) -> None:
        """Set task branch flow."""
        branch = await choose_branch_with_modal(
            self.screen.app,
            project_root=self.screen.kagan_app.project_root,
            current_value=task.base_branch or "",
            title="Set Task Branch",
            description=f"Set base branch for: {task.title[:40]}",
            timeout_seconds=BRANCH_LOOKUP_TIMEOUT_SECONDS,
            warn=lambda message: self.screen.notify(message, severity="warning"),
        )
        if branch is not None:
            await self.update_task_branch(task, branch)

    async def update_task_branch(self, task: Task, branch: str) -> None:
        await self.screen.ctx.api.update_task(task.id, base_branch=branch or None)
        await self.screen._board.refresh_board()
        self.screen.notify(f"Branch set to: {branch or '(default)'}")
