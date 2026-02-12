from __future__ import annotations

import asyncio
from datetime import UTC, datetime
from time import monotonic
from types import SimpleNamespace
from typing import Any, cast

import pytest

from kagan.core.adapters.db.repositories import JobRepository, TaskRepository
from kagan.core.api import KaganAPI
from kagan.core.request_handlers import handle_job_events
from kagan.core.services.jobs import JobServiceImpl, JobStatus


async def _create_service(
    tmp_path,
    executor,
) -> tuple[TaskRepository, JobRepository, JobServiceImpl]:
    task_repo = TaskRepository(tmp_path / "jobs.db")
    await task_repo.initialize()
    repository = JobRepository(task_repo.session_factory)
    service = JobServiceImpl(executor, repository=repository)
    return task_repo, repository, service


async def _wait_for_status(service: JobServiceImpl, job_id: str, status: JobStatus) -> None:
    deadline = monotonic() + 2.0
    while monotonic() < deadline:
        record = await service.get(job_id)
        if record is not None and record.status == status:
            return
        await asyncio.sleep(0.01)
    raise AssertionError(f"Timed out waiting for job {job_id} to reach {status.value}")


@pytest.mark.asyncio()
async def test_submit_job_runs_executor_and_marks_succeeded(tmp_path) -> None:
    calls: list[tuple[str, dict[str, object]]] = []

    async def executor(action: str, params: dict[str, object]) -> dict[str, object]:
        calls.append((action, params))
        return {"success": True, "message": "queued", "code": "START_QUEUED"}

    task_repo, _repository, service = await _create_service(tmp_path, executor)
    try:
        record = await service.submit(
            task_id="TASK-1",
            action="start_agent",
            params={"task_id": "TASK-1"},
        )
        await _wait_for_status(service, record.job_id, JobStatus.SUCCEEDED)
        final = await service.get(record.job_id)
        assert final is not None
        assert final.status == JobStatus.SUCCEEDED
        assert final.code == "START_QUEUED"
        assert calls == [("start_agent", {"task_id": "TASK-1"})]
    finally:
        await service.shutdown()
        await task_repo.close()


@pytest.mark.asyncio()
async def test_cancel_running_job_marks_cancelled(tmp_path) -> None:
    gate = asyncio.Event()

    async def executor(action: str, params: dict[str, object]) -> dict[str, object]:
        del action, params
        await gate.wait()
        return {"success": True}

    task_repo, _repository, service = await _create_service(tmp_path, executor)
    try:
        record = await service.submit(
            task_id="TASK-2",
            action="start_agent",
            params={"task_id": "TASK-2"},
        )
        await _wait_for_status(service, record.job_id, JobStatus.RUNNING)
        cancelled = await service.cancel(record.job_id, task_id="TASK-2")
        assert cancelled is not None
        assert cancelled.status == JobStatus.CANCELLED
        final = await service.get(record.job_id)
        assert final is not None
        assert final.status == JobStatus.CANCELLED
    finally:
        gate.set()
        await service.shutdown()
        await task_repo.close()


@pytest.mark.asyncio()
async def test_cancel_rejects_wrong_task_scope(tmp_path) -> None:
    async def executor(action: str, params: dict[str, object]) -> dict[str, object]:
        del action, params
        return {"success": True}

    task_repo, _repository, service = await _create_service(tmp_path, executor)
    try:
        record = await service.submit(
            task_id="TASK-3",
            action="stop_agent",
            params={"task_id": "TASK-3"},
        )
        cancelled = await service.cancel(record.job_id, task_id="TASK-OTHER")
        assert cancelled is None
    finally:
        await service.shutdown()
        await task_repo.close()


@pytest.mark.asyncio()
async def test_wait_returns_terminal_record_after_completion(tmp_path) -> None:
    async def executor(action: str, params: dict[str, object]) -> dict[str, object]:
        del action, params
        await asyncio.sleep(0.01)
        return {"success": True, "code": "STARTED"}

    task_repo, _repository, service = await _create_service(tmp_path, executor)
    try:
        record = await service.submit(
            task_id="TASK-4",
            action="start_agent",
            params={"task_id": "TASK-4"},
        )
        waited = await service.wait(record.job_id, task_id="TASK-4", timeout_seconds=1.0)
        assert waited is not None
        assert waited.status == JobStatus.SUCCEEDED
        assert waited.code == "STARTED"
    finally:
        await service.shutdown()
        await task_repo.close()


@pytest.mark.asyncio()
async def test_wait_times_out_when_job_is_still_running(tmp_path) -> None:
    gate = asyncio.Event()

    async def executor(action: str, params: dict[str, object]) -> dict[str, object]:
        del action, params
        await gate.wait()
        return {"success": True}

    task_repo, _repository, service = await _create_service(tmp_path, executor)
    try:
        record = await service.submit(
            task_id="TASK-5",
            action="start_agent",
            params={"task_id": "TASK-5"},
        )
        await _wait_for_status(service, record.job_id, JobStatus.RUNNING)
        waited = await service.wait(record.job_id, task_id="TASK-5", timeout_seconds=0.01)
        assert waited is not None
        assert waited.status == JobStatus.RUNNING
    finally:
        gate.set()
        await service.shutdown()
        await task_repo.close()


@pytest.mark.asyncio()
async def test_wait_with_zero_timeout_returns_current_state_immediately(tmp_path) -> None:
    gate = asyncio.Event()

    async def executor(action: str, params: dict[str, object]) -> dict[str, object]:
        del action, params
        await gate.wait()
        return {"success": True}

    task_repo, _repository, service = await _create_service(tmp_path, executor)
    try:
        record = await service.submit(
            task_id="TASK-6",
            action="start_agent",
            params={"task_id": "TASK-6"},
        )
        await _wait_for_status(service, record.job_id, JobStatus.RUNNING)
        started_at = monotonic()
        waited = await service.wait(record.job_id, task_id="TASK-6", timeout_seconds=0.0)
        elapsed = monotonic() - started_at
        assert waited is not None
        assert waited.status == JobStatus.RUNNING
        assert elapsed < 0.1
    finally:
        gate.set()
        await service.shutdown()
        await task_repo.close()


@pytest.mark.asyncio()
async def test_wait_returns_cancelled_record_when_cancelled_while_waiting(tmp_path) -> None:
    gate = asyncio.Event()

    async def executor(action: str, params: dict[str, object]) -> dict[str, object]:
        del action, params
        await gate.wait()
        return {"success": True}

    task_repo, _repository, service = await _create_service(tmp_path, executor)
    try:
        record = await service.submit(
            task_id="TASK-7",
            action="start_agent",
            params={"task_id": "TASK-7"},
        )
        await _wait_for_status(service, record.job_id, JobStatus.RUNNING)
        wait_task = asyncio.create_task(
            service.wait(record.job_id, task_id="TASK-7", timeout_seconds=1.0)
        )
        await asyncio.sleep(0.01)

        cancelled = await service.cancel(record.job_id, task_id="TASK-7")
        assert cancelled is not None
        assert cancelled.status == JobStatus.CANCELLED

        waited = await wait_task
        assert waited is not None
        assert waited.status == JobStatus.CANCELLED
        assert waited.code == "JOB_CANCELLED"
    finally:
        gate.set()
        await service.shutdown()
        await task_repo.close()


@pytest.mark.asyncio()
async def test_events_include_lifecycle_for_successful_job(tmp_path) -> None:
    async def executor(action: str, params: dict[str, object]) -> dict[str, object]:
        del action, params
        return {"success": True, "message": "done", "code": "DONE"}

    task_repo, _repository, service = await _create_service(tmp_path, executor)
    try:
        record = await service.submit(
            task_id="TASK-8",
            action="start_agent",
            params={"task_id": "TASK-8"},
        )
        await _wait_for_status(service, record.job_id, JobStatus.SUCCEEDED)

        events = await service.events(record.job_id, task_id="TASK-8")
        assert events is not None
        assert [event.status for event in events] == [
            JobStatus.QUEUED,
            JobStatus.RUNNING,
            JobStatus.SUCCEEDED,
        ]
        assert events[0].code == "JOB_QUEUED"
        assert events[1].code == "JOB_RUNNING"
        assert events[2].code == "DONE"
        assert events[2].message == "done"
        timestamps = [event.timestamp for event in events]
        assert timestamps == sorted(timestamps)
    finally:
        await service.shutdown()
        await task_repo.close()


@pytest.mark.asyncio()
async def test_events_include_single_cancelled_transition(tmp_path) -> None:
    gate = asyncio.Event()

    async def executor(action: str, params: dict[str, object]) -> dict[str, object]:
        del action, params
        await gate.wait()
        return {"success": True}

    task_repo, _repository, service = await _create_service(tmp_path, executor)
    try:
        record = await service.submit(
            task_id="TASK-9",
            action="start_agent",
            params={"task_id": "TASK-9"},
        )
        await _wait_for_status(service, record.job_id, JobStatus.RUNNING)
        cancelled = await service.cancel(record.job_id, task_id="TASK-9")
        assert cancelled is not None
        assert cancelled.status == JobStatus.CANCELLED

        events = await service.events(record.job_id, task_id="TASK-9")
        assert events is not None
        assert [event.status for event in events] == [
            JobStatus.QUEUED,
            JobStatus.RUNNING,
            JobStatus.CANCELLED,
        ]
        assert events[2].code == "JOB_CANCELLED"
        assert events[2].message == "Job cancelled"

        cancelled_again = await service.cancel(record.job_id, task_id="TASK-9")
        assert cancelled_again is not None
        assert cancelled_again.status == JobStatus.CANCELLED

        events_after_retry = await service.events(record.job_id, task_id="TASK-9")
        assert events_after_retry is not None
        assert [event.status for event in events_after_retry] == [
            JobStatus.QUEUED,
            JobStatus.RUNNING,
            JobStatus.CANCELLED,
        ]
    finally:
        gate.set()
        await service.shutdown()
        await task_repo.close()


@pytest.mark.asyncio()
async def test_job_state_persists_across_service_recreation(tmp_path) -> None:
    calls: list[tuple[str, dict[str, object]]] = []

    async def first_executor(action: str, params: dict[str, object]) -> dict[str, object]:
        calls.append((action, params))
        return {"success": True, "message": "done", "code": "DONE"}

    task_repo = TaskRepository(tmp_path / "jobs.db")
    await task_repo.initialize()
    repository_one = JobRepository(task_repo.session_factory)

    service_one = JobServiceImpl(first_executor, repository=repository_one)
    service_two: JobServiceImpl | None = None
    try:
        submitted = await service_one.submit(
            task_id="TASK-PERSIST",
            action="start_agent",
            params={"task_id": "TASK-PERSIST"},
        )
        awaited = await service_one.wait(
            submitted.job_id,
            task_id="TASK-PERSIST",
            timeout_seconds=1.0,
        )
        assert awaited is not None
        assert awaited.status == JobStatus.SUCCEEDED

        await service_one.shutdown()

        async def second_executor(action: str, params: dict[str, object]) -> dict[str, object]:
            del action, params
            msg = "recreated service should not execute completed jobs"
            raise AssertionError(msg)

        repository_two = JobRepository(task_repo.session_factory)
        service_two = JobServiceImpl(second_executor, repository=repository_two)

        restored = await service_two.get(submitted.job_id)
        assert restored is not None
        assert restored.status == JobStatus.SUCCEEDED
        assert restored.code == "DONE"

        restored_events = await service_two.events(submitted.job_id, task_id="TASK-PERSIST")
        assert restored_events is not None
        assert [event.status for event in restored_events] == [
            JobStatus.QUEUED,
            JobStatus.RUNNING,
            JobStatus.SUCCEEDED,
        ]
        assert calls == [("start_agent", {"task_id": "TASK-PERSIST"})]
    finally:
        if service_two is not None:
            await service_two.shutdown()
        await service_one.shutdown()
        await task_repo.close()


@pytest.mark.asyncio()
async def test_recovery_marks_stale_non_terminal_jobs_failed(tmp_path) -> None:
    task_repo = TaskRepository(tmp_path / "jobs.db")
    await task_repo.initialize()
    repository = JobRepository(task_repo.session_factory)

    stale_created_at = datetime.now(UTC)
    await repository.create_job(
        job_id="stale-job-1",
        task_id="TASK-RECOVER",
        action="start_agent",
        params_json={"task_id": "TASK-RECOVER"},
        created_at=stale_created_at,
        queued_message="Job queued",
        queued_code="JOB_QUEUED",
    )

    executor_calls: list[dict[str, Any]] = []

    async def executor(action: str, params: dict[str, object]) -> dict[str, object]:
        executor_calls.append({"action": action, "params": params})
        return {"success": True}

    service = JobServiceImpl(executor, repository=repository)
    try:
        recovered = await service.get("stale-job-1")
        assert recovered is not None
        assert recovered.status == JobStatus.FAILED
        assert recovered.code == "JOB_RECOVERED_INTERRUPTED"

        events = await service.events("stale-job-1", task_id="TASK-RECOVER")
        assert events is not None
        assert [event.status for event in events] == [JobStatus.QUEUED, JobStatus.FAILED]
        assert events[1].code == "JOB_RECOVERED_INTERRUPTED"
        assert executor_calls == []
    finally:
        await service.shutdown()
        await task_repo.close()


@pytest.mark.asyncio()
async def test_handle_job_events_paginates_persisted_lifecycle(tmp_path) -> None:
    async def executor(action: str, params: dict[str, object]) -> dict[str, object]:
        del action, params
        return {"success": True, "message": "done", "code": "DONE"}

    task_repo, _repository, service = await _create_service(tmp_path, executor)
    try:
        submitted = await service.submit(
            task_id="TASK-PAGE",
            action="start_agent",
            params={"task_id": "TASK-PAGE"},
        )
        await _wait_for_status(service, submitted.job_id, JobStatus.SUCCEEDED)

        f = KaganAPI(cast("Any", SimpleNamespace(job_service=service)))
        page = await handle_job_events(
            f,
            {
                "job_id": submitted.job_id,
                "task_id": "TASK-PAGE",
                "offset": 1,
                "limit": 1,
            },
        )

        assert page["success"] is True
        assert page["total_events"] == 3
        assert page["returned_events"] == 1
        assert page["offset"] == 1
        assert page["limit"] == 1
        assert page["has_more"] is True
        assert page["next_offset"] == 2
        assert [event["status"] for event in page["events"]] == ["running"]
    finally:
        await service.shutdown()
        await task_repo.close()
