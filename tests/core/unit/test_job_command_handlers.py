"""Tests for job api adapter functions (formerly CQRS handlers)."""

from __future__ import annotations

from datetime import UTC, datetime
from types import SimpleNamespace
from typing import Any, cast

import pytest

from kagan.core.api import KaganAPI
from kagan.core.request_handlers import (
    handle_job_cancel,
    handle_job_events,
    handle_job_get,
    handle_job_submit,
    handle_job_wait,
)
from kagan.core.services.jobs import JobEvent, JobRecord, JobStatus


def _api(**services: object) -> KaganAPI:
    ctx = SimpleNamespace(**services)
    return KaganAPI(cast("Any", ctx))


@pytest.mark.asyncio()
async def test_job_submit_requires_valid_task_id() -> None:
    f = _api()
    result = await handle_job_submit(f, {"action": "start_agent"})
    assert result["success"] is False
    assert result["code"] == "INVALID_TASK_ID"


@pytest.mark.asyncio()
async def test_job_submit_returns_job_metadata() -> None:
    now = datetime.now(UTC)
    submitted: list[tuple[str, str, dict[str, object] | None]] = []

    class _TaskService:
        async def get_task(self, task_id: str) -> SimpleNamespace:
            return SimpleNamespace(id=task_id)

    class _JobService:
        async def submit(
            self,
            *,
            task_id: str,
            action: str,
            params: dict[str, object] | None = None,
        ) -> JobRecord:
            submitted.append((task_id, action, params))
            return JobRecord(
                job_id="job-1",
                task_id=task_id,
                action=action,
                status=JobStatus.QUEUED,
                created_at=now,
                updated_at=now,
                params=dict(params or {}),
            )

    f = _api(task_service=_TaskService(), job_service=_JobService())
    result = await handle_job_submit(
        f,
        {"task_id": "TASK-1", "action": "start_agent", "arguments": {"dry_run": True}},
    )
    assert result["success"] is True
    assert result["job_id"] == "job-1"


@pytest.mark.asyncio()
async def test_job_cancel_requires_scope_task_id() -> None:
    f = _api()
    result = await handle_job_cancel(f, {"job_id": "job-1"})
    assert result["success"] is False
    assert result["code"] == "INVALID_TASK_ID"


@pytest.mark.asyncio()
async def test_job_get_returns_not_found_for_scope_mismatch() -> None:
    now = datetime.now(UTC)

    class _JobService:
        async def get(self, job_id: str, *, task_id: str | None = None) -> JobRecord | None:
            rec = JobRecord(
                job_id=job_id,
                task_id="TASK-1",
                action="start_agent",
                status=JobStatus.RUNNING,
                created_at=now,
                updated_at=now,
                params={"task_id": "TASK-1"},
            )
            if task_id is not None and rec.task_id != task_id:
                return None
            return rec

    f = _api(job_service=_JobService())
    result = await handle_job_get(f, {"job_id": "job-1", "task_id": "TASK-2"})
    assert result["success"] is False
    assert result["code"] == "JOB_NOT_FOUND"


@pytest.mark.asyncio()
async def test_job_wait_rejects_invalid_timeout() -> None:
    f = _api()
    result = await handle_job_wait(
        f,
        {"job_id": "job-1", "task_id": "TASK-1", "timeout_seconds": "soon"},
    )
    assert result["success"] is False
    assert result["code"] == "INVALID_TIMEOUT"


@pytest.mark.asyncio()
async def test_job_wait_marks_timeout_for_non_terminal_status() -> None:
    now = datetime.now(UTC)

    class _JobService:
        async def wait(
            self,
            job_id: str,
            *,
            task_id: str,
            timeout_seconds: float | None = None,
        ) -> JobRecord:
            del timeout_seconds
            return JobRecord(
                job_id=job_id,
                task_id=task_id,
                action="start_agent",
                status=JobStatus.RUNNING,
                created_at=now,
                updated_at=now,
                params={"task_id": task_id},
            )

    f = _api(job_service=_JobService())
    result = await handle_job_wait(
        f,
        {"job_id": "job-2", "task_id": "TASK-9", "timeout_seconds": 0.01},
    )
    assert result["success"] is True
    assert result["status"] == "running"
    assert result["timed_out"] is True
    assert result["code"] == "JOB_TIMEOUT"


@pytest.mark.asyncio()
async def test_job_events_rejects_invalid_limit() -> None:
    f = _api()
    result = await handle_job_events(
        f,
        {"job_id": "job-2", "task_id": "TASK-9", "limit": 0},
    )
    assert result["success"] is False
    assert result["code"] == "INVALID_LIMIT"


@pytest.mark.asyncio()
async def test_job_events_returns_bounded_page() -> None:
    now = datetime.now(UTC)

    class _JobService:
        async def events(self, job_id: str, *, task_id: str) -> list[JobEvent]:
            return [
                JobEvent(
                    job_id=job_id,
                    task_id=task_id,
                    status=JobStatus.QUEUED,
                    timestamp=now,
                    message="Job queued",
                    code="JOB_QUEUED",
                ),
                JobEvent(
                    job_id=job_id,
                    task_id=task_id,
                    status=JobStatus.RUNNING,
                    timestamp=now,
                    message="Job running",
                    code="JOB_RUNNING",
                ),
                JobEvent(
                    job_id=job_id,
                    task_id=task_id,
                    status=JobStatus.SUCCEEDED,
                    timestamp=now,
                    message="done",
                    code="DONE",
                ),
            ]

    f = _api(job_service=_JobService())
    result = await handle_job_events(
        f,
        {"job_id": "job-2", "task_id": "TASK-9", "offset": 1, "limit": 2},
    )
    assert result["success"] is True
    assert result["total_events"] == 3
    assert result["returned_events"] == 2
    assert result["offset"] == 1
    assert result["limit"] == 2
    assert result["has_more"] is False
    assert result["next_offset"] is None
    assert [event["status"] for event in result["events"]] == ["running", "succeeded"]
