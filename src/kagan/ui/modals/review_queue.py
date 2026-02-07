"""Queued follow-up handling for the review modal."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

from kagan.core.models.enums import StreamPhase, TaskStatus, TaskType
from kagan.ui.modals.review_prompt import truncate_queue_payload

if TYPE_CHECKING:
    from kagan.app import KaganApp
    from kagan.services.queued_messages import QueuedMessageService
    from kagan.ui.modals.review import ReviewModal


class ReviewQueueMixin:
    """Queue APIs and queue-aware UI state updates."""

    async def _configure_agent_output_chat(self: ReviewModal) -> None:
        panel = self._get_agent_output_panel()
        panel.set_send_handler(self._send_implementation_follow_up)
        panel.set_get_queued_handler(self._get_implementation_queued_messages)
        panel.set_remove_handler(self._remove_implementation_queued_message)
        await panel.refresh_queued_messages()
        self._sync_agent_output_queue_visibility()

    def _sync_agent_output_queue_visibility(self: ReviewModal) -> None:
        enabled = self._task_model.task_type == TaskType.AUTO and (
            self._task_model.status == TaskStatus.IN_PROGRESS
        )
        panel = self._get_agent_output_panel()
        if enabled:
            panel.remove_class("queue-disabled")
        else:
            panel.add_class("queue-disabled")

    async def _configure_follow_up_chat(self: ReviewModal) -> None:
        panel = self._get_chat_panel()
        panel.set_send_handler(self._send_follow_up)
        panel.set_get_queued_handler(self._get_review_queued_messages)
        panel.set_remove_handler(self._remove_review_queued_message)
        await panel.refresh_queued_messages()
        self._sync_review_queue_visibility()

    def _sync_review_queue_visibility(self: ReviewModal) -> None:
        enabled = self._task_model.status == TaskStatus.REVIEW and not self._read_only
        panel = self._get_chat_panel()
        if enabled:
            panel.remove_class("queue-disabled")
        else:
            panel.add_class("queue-disabled")

    async def _refresh_review_queue_state(self: ReviewModal) -> None:
        self._sync_review_queue_visibility()
        await self._get_chat_panel().refresh_queued_messages()
        if self._task_model.status != TaskStatus.REVIEW:
            self._review_queue_pending = False
            self._sync_review_action_state()
            return
        service = self._get_queue_service()
        if service is None:
            self._review_queue_pending = False
            self._sync_review_action_state()
            return
        status = await service.get_status(self._task_model.id, lane="review")
        self._review_queue_pending = status.has_queued
        self._sync_review_action_state()

    async def _refresh_implementation_queue_state(self: ReviewModal) -> None:
        self._sync_agent_output_queue_visibility()
        await self._get_agent_output_panel().refresh_queued_messages()
        if self._task_model.status != TaskStatus.IN_PROGRESS:
            self._implementation_queue_pending = False
            return
        service = self._get_queue_service()
        if service is None:
            self._implementation_queue_pending = False
            return
        status = await service.get_status(self._task_model.id, lane="implementation")
        self._implementation_queue_pending = status.has_queued

    def _get_queue_service(self: ReviewModal) -> QueuedMessageService | None:
        app = cast("KaganApp", self.app)
        service = getattr(app.ctx, "queued_message_service", None)
        if service is None:
            return None
        return cast("QueuedMessageService", service)

    async def _send_follow_up(self: ReviewModal, content: str) -> None:
        if self._task_model.status != TaskStatus.REVIEW:
            raise RuntimeError("Review queue only available in REVIEW status")
        service = self._get_queue_service()
        if service is None:
            raise RuntimeError("Follow-up queue unavailable")
        await service.queue_message(self._task_model.id, content, lane="review")
        await self._refresh_review_queue_state()
        if self._phase == StreamPhase.IDLE and not self._live_review_attached:
            await self._start_review_follow_up_if_needed()

    async def _get_review_queued_messages(self: ReviewModal) -> list:
        service = self._get_queue_service()
        if service is None:
            return []
        return await service.get_queued(self._task_model.id, lane="review")

    async def _remove_review_queued_message(self: ReviewModal, index: int) -> bool:
        service = self._get_queue_service()
        if service is None:
            return False
        removed = await service.remove_message(self._task_model.id, index, lane="review")
        await self._refresh_review_queue_state()
        return removed

    async def _take_review_queue(self: ReviewModal) -> str | None:
        service = self._get_queue_service()
        if service is None:
            return None
        queued = await service.take_queued(self._task_model.id, lane="review")
        await self._refresh_review_queue_state()
        if queued is None:
            return None
        return truncate_queue_payload(queued.content)

    async def _send_implementation_follow_up(self: ReviewModal, content: str) -> None:
        if self._task_model.status != TaskStatus.IN_PROGRESS:
            raise RuntimeError("Implementation queue only available in IN_PROGRESS")
        service = self._get_queue_service()
        if service is None:
            raise RuntimeError("Implementation queue unavailable")
        await service.queue_message(self._task_model.id, content, lane="implementation")
        await self._refresh_implementation_queue_state()

        app = cast("KaganApp", self.app)
        automation = app.ctx.automation_service
        if not automation.is_running(self._task_model.id):
            await automation.spawn_for_task(self._task_model)
            await self._get_agent_output_panel().output.post_note(
                "Queued follow-up accepted. Starting next implementation run...",
                classes="info",
            )
            await self._refresh_runtime_state()

    async def _get_implementation_queued_messages(self: ReviewModal) -> list:
        service = self._get_queue_service()
        if service is None:
            return []
        return await service.get_queued(self._task_model.id, lane="implementation")

    async def _remove_implementation_queued_message(self: ReviewModal, index: int) -> bool:
        service = self._get_queue_service()
        if service is None:
            return False
        removed = await service.remove_message(self._task_model.id, index, lane="implementation")
        await self._refresh_implementation_queue_state()
        return removed

    async def _start_review_follow_up_if_needed(self: ReviewModal) -> None:
        if self._phase not in (StreamPhase.IDLE, StreamPhase.COMPLETE):
            return
        service = self._get_queue_service()
        if service is None:
            return
        status = await service.get_status(self._task_model.id, lane="review")
        if not status.has_queued:
            return
        chat_panel = self._get_chat_panel()
        chat_panel.remove_class("hidden")
        await chat_panel.output.post_note("Starting queued review follow-up...", classes="info")
        self._set_decision(None)
        self._set_phase(StreamPhase.THINKING)
        await self._generate_ai_review(chat_panel.output)
