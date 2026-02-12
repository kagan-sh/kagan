from __future__ import annotations

import asyncio
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Final

from sqlalchemy import func
from sqlmodel import col, select

from kagan.core.adapters.db.schema import Job, JobAttempt, JobEventRecord

if TYPE_CHECKING:
    from collections.abc import Sequence
    from datetime import datetime

    from sqlalchemy.ext.asyncio import AsyncSession

    from kagan.core.adapters.db.repositories.base import ClosingAwareSessionFactory

_JOB_STATUS_QUEUED: Final = "queued"
_JOB_STATUS_RUNNING: Final = "running"
_JOB_STATUS_SUCCEEDED: Final = "succeeded"
_JOB_STATUS_FAILED: Final = "failed"
_JOB_STATUS_CANCELLED: Final = "cancelled"
_JOB_EVENT_INDEX_INITIAL: Final = 1
_TERMINAL_JOB_STATUSES: frozenset[str] = frozenset(
    {_JOB_STATUS_SUCCEEDED, _JOB_STATUS_FAILED, _JOB_STATUS_CANCELLED}
)
_NON_TERMINAL_JOB_STATUSES: frozenset[str] = frozenset({_JOB_STATUS_QUEUED, _JOB_STATUS_RUNNING})


@dataclass(frozen=True, slots=True)
class JobTransition:
    """Result of a transition attempt on a job lifecycle state."""

    job: Job
    transitioned: bool


class JobRepository:
    """Repository for durable jobs, lifecycle events, and attempts."""

    def __init__(self, session_factory: ClosingAwareSessionFactory) -> None:
        self._session_factory = session_factory
        self._lock = asyncio.Lock()

    def _get_session(self) -> AsyncSession:
        return self._session_factory()

    async def create_job(
        self,
        *,
        job_id: str,
        task_id: str,
        action: str,
        params_json: dict[str, Any],
        created_at: datetime,
        queued_message: str,
        queued_code: str,
    ) -> Job:
        """Create a queued job and its initial lifecycle event."""
        async with self._lock:
            async with self._get_session() as session:
                job = Job(
                    id=job_id,
                    task_id=task_id,
                    action=action,
                    status=_JOB_STATUS_QUEUED,
                    params_json=params_json,
                    created_at=created_at,
                    updated_at=created_at,
                )
                session.add(job)
                session.add(
                    JobEventRecord(
                        job_id=job_id,
                        task_id=task_id,
                        event_index=_JOB_EVENT_INDEX_INITIAL,
                        status=_JOB_STATUS_QUEUED,
                        message=queued_message,
                        code=queued_code,
                        created_at=created_at,
                    )
                )
                await session.commit()
                return job

    async def get_job(self, job_id: str) -> Job | None:
        """Return a job by ID."""
        async with self._get_session() as session:
            return await session.get(Job, job_id)

    async def list_events(self, job_id: str) -> list[JobEventRecord]:
        """Return all lifecycle events for a job in chronological order."""
        async with self._get_session() as session:
            result = await session.execute(
                select(JobEventRecord)
                .where(JobEventRecord.job_id == job_id)
                .order_by(
                    col(JobEventRecord.event_index).asc(),
                    col(JobEventRecord.created_at).asc(),
                    col(JobEventRecord.id).asc(),
                )
            )
            return list(result.scalars().all())

    async def list_non_terminal_jobs(self) -> list[Job]:
        """Return jobs that were left queued/running and need recovery."""
        async with self._get_session() as session:
            result = await session.execute(
                select(Job)
                .where(col(Job.status).in_(_NON_TERMINAL_JOB_STATUSES))
                .order_by(col(Job.created_at).asc(), col(Job.id).asc())
            )
            return list(result.scalars().all())

    async def mark_running(
        self,
        job_id: str,
        *,
        timestamp: datetime,
        message: str,
        code: str,
    ) -> JobTransition | None:
        """Transition a queued job to running and start a new attempt."""
        async with self._lock:
            async with self._get_session() as session:
                job = await session.get(Job, job_id)
                if job is None:
                    return None
                if job.status != _JOB_STATUS_QUEUED:
                    return JobTransition(job=job, transitioned=False)

                job.status = _JOB_STATUS_RUNNING
                job.updated_at = timestamp
                job.message = message
                job.code = code
                job.last_attempt_number += 1
                session.add(job)

                session.add(
                    JobAttempt(
                        job_id=job.id,
                        attempt_number=job.last_attempt_number,
                        status=_JOB_STATUS_RUNNING,
                        started_at=timestamp,
                    )
                )

                session.add(
                    JobEventRecord(
                        job_id=job.id,
                        task_id=job.task_id,
                        event_index=await self._next_event_index(session, job.id),
                        status=_JOB_STATUS_RUNNING,
                        message=message,
                        code=code,
                        created_at=timestamp,
                    )
                )
                await session.commit()
                return JobTransition(job=job, transitioned=True)

    async def complete_job(
        self,
        job_id: str,
        *,
        status: str,
        timestamp: datetime,
        message: str | None,
        code: str | None,
        result_json: dict[str, Any] | None,
    ) -> JobTransition | None:
        """Transition job to a terminal state and append final lifecycle event."""
        if status not in _TERMINAL_JOB_STATUSES:
            msg = f"Terminal status required, got '{status}'"
            raise ValueError(msg)

        async with self._lock:
            async with self._get_session() as session:
                job = await session.get(Job, job_id)
                if job is None:
                    return None
                if job.status in _TERMINAL_JOB_STATUSES:
                    return JobTransition(job=job, transitioned=False)

                job.status = status
                job.updated_at = timestamp
                job.finished_at = timestamp
                job.message = message
                job.code = code
                if result_json is not None:
                    job.result_json = result_json
                session.add(job)

                attempt = await self._latest_attempt(session, job_id)
                if attempt is not None and attempt.finished_at is None:
                    attempt.status = status
                    attempt.finished_at = timestamp
                    attempt.message = message
                    attempt.code = code
                    attempt.result_json = result_json
                    session.add(attempt)

                session.add(
                    JobEventRecord(
                        job_id=job.id,
                        task_id=job.task_id,
                        event_index=await self._next_event_index(session, job.id),
                        status=status,
                        message=message,
                        code=code,
                        created_at=timestamp,
                    )
                )
                await session.commit()
                return JobTransition(job=job, transitioned=True)

    async def recover_non_terminal_jobs(
        self,
        *,
        timestamp: datetime,
        message: str,
        code: str,
        result_json: dict[str, Any],
    ) -> list[Job]:
        """Fail all queued/running jobs left behind by previous service instances."""
        async with self._lock:
            async with self._get_session() as session:
                result = await session.execute(
                    select(Job)
                    .where(col(Job.status).in_(_NON_TERMINAL_JOB_STATUSES))
                    .order_by(col(Job.created_at).asc(), col(Job.id).asc())
                )
                stale_jobs = list(result.scalars().all())
                if not stale_jobs:
                    return []

                job_ids = tuple(job.id for job in stale_jobs)
                latest_attempts = await self._latest_attempts_by_job_id(session, job_ids)
                next_event_indices = await self._next_event_indices_by_job_id(session, job_ids)

                for job in stale_jobs:
                    job.status = _JOB_STATUS_FAILED
                    job.updated_at = timestamp
                    job.finished_at = timestamp
                    job.message = message
                    job.code = code
                    job.result_json = result_json
                    session.add(job)

                    attempt = latest_attempts.get(job.id)
                    if attempt is not None and attempt.finished_at is None:
                        attempt.status = _JOB_STATUS_FAILED
                        attempt.finished_at = timestamp
                        attempt.message = message
                        attempt.code = code
                        attempt.result_json = result_json
                        session.add(attempt)

                    event_index = next_event_indices.get(job.id, _JOB_EVENT_INDEX_INITIAL)
                    next_event_indices[job.id] = event_index + 1
                    session.add(
                        JobEventRecord(
                            job_id=job.id,
                            task_id=job.task_id,
                            event_index=event_index,
                            status=_JOB_STATUS_FAILED,
                            message=message,
                            code=code,
                            created_at=timestamp,
                        )
                    )

                await session.commit()
                return stale_jobs

    async def _next_event_index(self, session: AsyncSession, job_id: str) -> int:
        result = await session.execute(
            select(func.max(JobEventRecord.event_index)).where(JobEventRecord.job_id == job_id)
        )
        value = result.scalar_one()
        return _JOB_EVENT_INDEX_INITIAL if value is None else int(value) + 1

    async def _next_event_indices_by_job_id(
        self, session: AsyncSession, job_ids: Sequence[str]
    ) -> dict[str, int]:
        if not job_ids:
            return {}
        result = await session.execute(
            select(JobEventRecord.job_id, func.max(JobEventRecord.event_index))
            .where(col(JobEventRecord.job_id).in_(job_ids))
            .group_by(JobEventRecord.job_id)
        )
        next_indices = {job_id: _JOB_EVENT_INDEX_INITIAL for job_id in job_ids}
        for job_id, max_index in result.all():
            next_indices[job_id] = (
                _JOB_EVENT_INDEX_INITIAL if max_index is None else int(max_index) + 1
            )
        return next_indices

    async def _latest_attempt(self, session: AsyncSession, job_id: str) -> JobAttempt | None:
        result = await session.execute(
            select(JobAttempt)
            .where(JobAttempt.job_id == job_id)
            .order_by(col(JobAttempt.attempt_number).desc(), col(JobAttempt.id).desc())
            .limit(1)
        )
        return result.scalars().first()

    async def _latest_attempts_by_job_id(
        self, session: AsyncSession, job_ids: Sequence[str]
    ) -> dict[str, JobAttempt]:
        if not job_ids:
            return {}
        result = await session.execute(
            select(JobAttempt)
            .where(col(JobAttempt.job_id).in_(job_ids))
            .order_by(
                col(JobAttempt.job_id).asc(),
                col(JobAttempt.attempt_number).desc(),
                col(JobAttempt.id).desc(),
            )
        )
        latest_attempts: dict[str, JobAttempt] = {}
        for attempt in result.scalars().all():
            if attempt.job_id not in latest_attempts:
                latest_attempts[attempt.job_id] = attempt
        return latest_attempts


__all__ = ["JobRepository", "JobTransition"]
