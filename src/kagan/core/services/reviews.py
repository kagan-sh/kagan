"""Review service for manual approval flow."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Protocol

from kagan.core.models.enums import TaskStatus
from kagan.core.time import utc_now

if TYPE_CHECKING:
    from kagan.core.adapters.db.repositories import ExecutionRepository
    from kagan.core.services.tasks import TaskService
    from kagan.core.services.types import ExecutionId, TaskId


class ReviewService(Protocol):
    """Service interface for review operations."""

    async def start_review(self, task_id: TaskId, execution_id: ExecutionId) -> None:
        """Start a review run for a task and execution."""
        ...

    async def record_review_response(
        self,
        task_id: TaskId,
        execution_id: ExecutionId,
        *,
        approved: bool,
        summary: str,
    ) -> None:
        """Record review response details."""
        ...


class ReviewServiceImpl:
    """Review service backed by TaskService and ExecutionRepository."""

    def __init__(
        self,
        task_service: TaskService,
        execution_service: ExecutionRepository,
    ) -> None:
        self._tasks = task_service
        self._executions = execution_service

    async def start_review(self, task_id: TaskId, execution_id: ExecutionId) -> None:
        await self._tasks.set_status(task_id, TaskStatus.REVIEW)
        note = f"Review requested for task {task_id}."
        await self._executions.append_execution_log(execution_id, self._serialize_note(note))

    async def record_review_response(
        self,
        task_id: TaskId,
        execution_id: ExecutionId,
        *,
        approved: bool,
        summary: str,
    ) -> None:
        status_label = "approved" if approved else "rejected"
        note = f"Review {status_label}: {summary}" if summary else f"Review {status_label}."
        await self._executions.append_execution_log(execution_id, self._serialize_note(note))

        await self._update_execution_metadata(execution_id, approved, summary)

        if summary:
            scratchpad = await self._tasks.get_scratchpad(task_id)
            header = f"\n\n--- REVIEW ({status_label.upper()}) ---\n"
            await self._tasks.update_scratchpad(task_id, scratchpad + header + summary)

    async def _update_execution_metadata(
        self,
        execution_id: ExecutionId,
        approved: bool,
        summary: str,
    ) -> None:
        execution = await self._executions.get_execution(execution_id)
        metadata: dict[str, object] = {}
        if execution is not None:
            metadata = dict(execution.metadata_ or {})
        metadata["review_result"] = {
            "approved": approved,
            "summary": summary,
            "completed_at": utc_now().isoformat(),
        }
        await self._executions.update_execution(execution_id, metadata=metadata)

    def _serialize_note(self, note: str) -> str:
        return json.dumps(
            {
                "messages": [
                    {
                        "type": "response",
                        "content": note,
                    }
                ]
            }
        )
