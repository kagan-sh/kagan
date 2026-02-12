from __future__ import annotations

import asyncio
import contextlib
import logging
import uuid
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from datetime import UTC, datetime
from enum import StrEnum
from typing import TYPE_CHECKING, Any, Protocol

import anyio

from kagan.core.adapters.db.repositories.base import RepositoryClosing

if TYPE_CHECKING:
    from kagan.core.adapters.db.repositories.jobs import JobRepository
    from kagan.core.adapters.db.schema import Job as JobModel
    from kagan.core.adapters.db.schema import JobEventRecord

log = logging.getLogger(__name__)

type JobExecutor = Callable[[str, dict[str, Any]], Awaitable[dict[str, Any]]]


class JobStatus(StrEnum):
    QUEUED = "queued"
    RUNNING = "running"
    SUCCEEDED = "succeeded"
    FAILED = "failed"
    CANCELLED = "cancelled"


_TERMINAL_JOB_STATUSES: frozenset[JobStatus] = frozenset(
    {JobStatus.SUCCEEDED, JobStatus.FAILED, JobStatus.CANCELLED}
)


@dataclass(slots=True)
class JobEvent:
    job_id: str
    task_id: str
    status: JobStatus
    timestamp: datetime
    message: str | None = None
    code: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "job_id": self.job_id,
            "task_id": self.task_id,
            "status": self.status.value,
            "timestamp": self.timestamp.isoformat(),
            "message": self.message,
            "code": self.code,
        }


@dataclass(slots=True)
class JobRecord:
    job_id: str
    task_id: str
    action: str
    status: JobStatus
    created_at: datetime
    updated_at: datetime
    params: dict[str, Any]
    result: dict[str, Any] | None = None
    message: str | None = None
    code: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "job_id": self.job_id,
            "task_id": self.task_id,
            "action": self.action,
            "status": self.status.value,
            "created_at": self.created_at.isoformat(),
            "updated_at": self.updated_at.isoformat(),
            "message": self.message,
            "code": self.code,
            "result": self.result,
        }


class JobService(Protocol):
    async def submit(
        self,
        *,
        task_id: str,
        action: str,
        params: dict[str, Any] | None = None,
    ) -> JobRecord: ...

    async def get(self, job_id: str) -> JobRecord | None: ...

    async def events(self, job_id: str, *, task_id: str) -> list[JobEvent] | None: ...

    async def wait(
        self,
        job_id: str,
        *,
        task_id: str,
        timeout_seconds: float | None = None,
    ) -> JobRecord | None: ...

    async def cancel(self, job_id: str, *, task_id: str) -> JobRecord | None: ...

    async def shutdown(self) -> None: ...


class JobServiceImpl:
    """DB-backed job runner with in-process AnyIO synchronization primitives."""

    def __init__(self, executor: JobExecutor, *, repository: JobRepository) -> None:
        self._executor = executor
        self._repository = repository
        self._tasks: dict[str, asyncio.Task[None]] = {}
        self._terminal_events: dict[str, anyio.Event] = {}
        self._lock = anyio.Lock()
        self._startup_recovered = False

    async def submit(
        self,
        *,
        task_id: str,
        action: str,
        params: dict[str, Any] | None = None,
    ) -> JobRecord:
        await self._ensure_recovered()
        now = datetime.now(UTC)
        job_id = str(uuid.uuid4())

        async with self._lock:
            job = await self._repository.create_job(
                job_id=job_id,
                task_id=task_id,
                action=action,
                params_json=dict(params or {}),
                created_at=now,
                queued_message="Job queued",
                queued_code="JOB_QUEUED",
            )
            self._terminal_events[job_id] = anyio.Event()
            self._tasks[job_id] = asyncio.create_task(
                self._run_job_in_task_group(job_id),
                name=f"job-{job_id}",
            )
            return self._job_to_record(job)

    async def get(self, job_id: str) -> JobRecord | None:
        await self._ensure_recovered()
        job = await self._repository.get_job(job_id)
        return None if job is None else self._job_to_record(job)

    async def events(self, job_id: str, *, task_id: str) -> list[JobEvent] | None:
        await self._ensure_recovered()
        job = await self._repository.get_job(job_id)
        if job is None or job.task_id != task_id:
            return None

        events = await self._repository.list_events(job_id)
        return [self._event_to_record(event) for event in events]

    async def wait(
        self,
        job_id: str,
        *,
        task_id: str,
        timeout_seconds: float | None = None,
    ) -> JobRecord | None:
        await self._ensure_recovered()

        wait_event: anyio.Event | None = None
        async with self._lock:
            job = await self._repository.get_job(job_id)
            if job is None or job.task_id != task_id:
                return None

            status = self._status_from_value(job.status)
            if status in _TERMINAL_JOB_STATUSES:
                return self._job_to_record(job)

            wait_event = self._terminal_events.get(job_id)
            if wait_event is None and job_id not in self._tasks:
                transition = await self._repository.complete_job(
                    job_id,
                    status=JobStatus.FAILED.value,
                    timestamp=datetime.now(UTC),
                    message="Job runner was not active for this in-flight job",
                    code="JOB_RUNNER_MISSING",
                    result_json={
                        "success": False,
                        "message": "Job runner was not active for this in-flight job",
                        "code": "JOB_RUNNER_MISSING",
                    },
                )
                if transition is not None:
                    if transition.transitioned:
                        self._set_terminal_event_locked(job_id)
                    return self._job_to_record(transition.job)

            wait_event = self._terminal_events.get(job_id)
            if wait_event is None:
                wait_event = anyio.Event()
                self._terminal_events[job_id] = wait_event

        if timeout_seconds is None:
            await wait_event.wait()
        else:
            with anyio.move_on_after(timeout_seconds):
                await wait_event.wait()

        job = await self._repository.get_job(job_id)
        if job is None or job.task_id != task_id:
            return None
        return self._job_to_record(job)

    async def cancel(self, job_id: str, *, task_id: str) -> JobRecord | None:
        await self._ensure_recovered()

        task: asyncio.Task[None] | None = None
        record: JobRecord | None = None
        async with self._lock:
            job = await self._repository.get_job(job_id)
            if job is None or job.task_id != task_id:
                return None

            status = self._status_from_value(job.status)
            if status in _TERMINAL_JOB_STATUSES:
                return self._job_to_record(job)

            transition = await self._repository.complete_job(
                job_id,
                status=JobStatus.CANCELLED.value,
                timestamp=datetime.now(UTC),
                message="Job cancelled",
                code="JOB_CANCELLED",
                result_json={
                    "success": False,
                    "message": "Job cancelled",
                    "code": "JOB_CANCELLED",
                },
            )
            if transition is None:
                return None

            if transition.transitioned:
                self._set_terminal_event_locked(job_id)
            task = self._tasks.get(job_id)
            record = self._job_to_record(transition.job)

        if task is not None and not task.done():
            task.cancel()
            cancelled_exc_class = anyio.get_cancelled_exc_class()
            with contextlib.suppress(cancelled_exc_class, asyncio.CancelledError):
                await task

        return record

    async def shutdown(self) -> None:
        await self._ensure_recovered()
        async with self._lock:
            tasks = list(self._tasks.values())

        if not tasks:
            return

        async with anyio.create_task_group() as task_group:
            for task in tasks:
                task_group.start_soon(self._cancel_and_wait_task, task)

    async def _ensure_recovered(self) -> None:
        if self._startup_recovered:
            return

        async with self._lock:
            if self._startup_recovered:
                return

            try:
                recovered_jobs = await self._repository.recover_non_terminal_jobs(
                    timestamp=datetime.now(UTC),
                    message="Job interrupted by previous service shutdown",
                    code="JOB_RECOVERED_INTERRUPTED",
                    result_json={
                        "success": False,
                        "message": "Job interrupted by previous service shutdown",
                        "code": "JOB_RECOVERED_INTERRUPTED",
                    },
                )
            except RepositoryClosing:
                self._startup_recovered = True
                return

            for job in recovered_jobs:
                self._set_terminal_event_locked(job.id)
            self._startup_recovered = True

    async def _run_job_in_task_group(self, job_id: str) -> None:
        # AnyIO task groups run on asyncio in this project and provide structured task lifecycle.
        async with anyio.create_task_group() as task_group:
            task_group.start_soon(self._run_job, job_id)

    async def _cancel_and_wait_task(self, task: asyncio.Task[None]) -> None:
        cancelled_exc_class = anyio.get_cancelled_exc_class()
        if not task.done():
            task.cancel()

        try:
            await task
        except (cancelled_exc_class, asyncio.CancelledError):
            return
        except Exception:
            log.exception("Job task raised during shutdown")

    async def _run_job(self, job_id: str) -> None:
        cancelled_exc_class = anyio.get_cancelled_exc_class()
        action = ""
        params: dict[str, Any] = {}

        try:
            async with self._lock:
                transition = await self._repository.mark_running(
                    job_id,
                    timestamp=datetime.now(UTC),
                    message="Job running",
                    code="JOB_RUNNING",
                )
                if transition is None:
                    self._tasks.pop(job_id, None)
                    return
                if not transition.transitioned:
                    status = self._status_from_value(transition.job.status)
                    if status in _TERMINAL_JOB_STATUSES:
                        self._set_terminal_event_locked(job_id)
                    return

                action = transition.job.action
                params = dict(transition.job.params_json)

            result = await self._executor(action, params)
            success = bool(result.get("success", False))
            message = str(result.get("message")) if result.get("message") else None
            code = str(result.get("code")) if result.get("code") else None
            terminal_status = JobStatus.SUCCEEDED if success else JobStatus.FAILED

            async with self._lock:
                transition = await self._repository.complete_job(
                    job_id,
                    status=terminal_status.value,
                    timestamp=datetime.now(UTC),
                    message=message,
                    code=code,
                    result_json=result,
                )
                if transition is not None and transition.transitioned:
                    self._set_terminal_event_locked(job_id)
        except cancelled_exc_class:
            # Shield persistence cleanup so cancellation cannot interrupt DB teardown.
            with anyio.CancelScope(shield=True):
                async with self._lock:
                    transition = await self._repository.complete_job(
                        job_id,
                        status=JobStatus.CANCELLED.value,
                        timestamp=datetime.now(UTC),
                        message="Job cancelled",
                        code="JOB_CANCELLED",
                        result_json={
                            "success": False,
                            "message": "Job cancelled",
                            "code": "JOB_CANCELLED",
                        },
                    )
                    if transition is not None and transition.transitioned:
                        self._set_terminal_event_locked(job_id)
            raise
        except RepositoryClosing:
            log.debug("Repository is closing; skipping job persistence for %s", job_id)
            async with self._lock:
                self._set_terminal_event_locked(job_id)
        except Exception as exc:  # quality-allow-broad-except
            log.exception("Job execution failed for %s", job_id)
            async with self._lock:
                try:
                    transition = await self._repository.complete_job(
                        job_id,
                        status=JobStatus.FAILED.value,
                        timestamp=datetime.now(UTC),
                        message=str(exc),
                        code="JOB_EXECUTION_ERROR",
                        result_json={
                            "success": False,
                            "message": str(exc),
                            "code": "JOB_EXECUTION_ERROR",
                        },
                    )
                except RepositoryClosing:
                    transition = None
                if transition is not None and transition.transitioned:
                    self._set_terminal_event_locked(job_id)
        finally:
            # Ensure bookkeeping always completes even if cancellation is still active.
            with anyio.CancelScope(shield=True):
                async with self._lock:
                    self._tasks.pop(job_id, None)

    @staticmethod
    def _status_from_value(value: str) -> JobStatus:
        return JobStatus(value)

    def _set_terminal_event_locked(self, job_id: str) -> None:
        terminal_event = self._terminal_events.pop(job_id, None)
        if terminal_event is not None:
            terminal_event.set()

    def _job_to_record(self, job: JobModel) -> JobRecord:
        return JobRecord(
            job_id=job.id,
            task_id=job.task_id,
            action=job.action,
            status=self._status_from_value(job.status),
            created_at=job.created_at,
            updated_at=job.updated_at,
            params=dict(job.params_json),
            result=dict(job.result_json) if job.result_json is not None else None,
            message=job.message,
            code=job.code,
        )

    def _event_to_record(self, event: JobEventRecord) -> JobEvent:
        return JobEvent(
            job_id=event.job_id,
            task_id=event.task_id,
            status=self._status_from_value(event.status),
            timestamp=event.created_at,
            message=event.message,
            code=event.code,
        )


__all__ = [
    "JobEvent",
    "JobRecord",
    "JobService",
    "JobServiceImpl",
    "JobStatus",
]
