"""Async repositories for domain entities."""

from __future__ import annotations

from kagan.core.adapters.db.repositories.auxiliary import (
    AuditRepository,
    PlannerRepository,
    RepoRepository,
    ScratchRepository,
    SessionRecordRepository,
)
from kagan.core.adapters.db.repositories.base import ClosingAwareSessionFactory, RepositoryClosing
from kagan.core.adapters.db.repositories.execution import ExecutionRepository
from kagan.core.adapters.db.repositories.jobs import JobRepository
from kagan.core.adapters.db.repositories.task import TaskRepository

__all__ = [
    "AuditRepository",
    "ClosingAwareSessionFactory",
    "ExecutionRepository",
    "JobRepository",
    "PlannerRepository",
    "RepoRepository",
    "RepositoryClosing",
    "ScratchRepository",
    "SessionRecordRepository",
    "TaskRepository",
]
