"""Async repositories for domain entities."""

from __future__ import annotations

from kagan.adapters.db.repositories.base import ClosingAwareSessionFactory, RepositoryClosing
from kagan.adapters.db.repositories.execution import ExecutionRepository
from kagan.adapters.db.repositories.repo import RepoRepository
from kagan.adapters.db.repositories.scratch import ScratchRepository
from kagan.adapters.db.repositories.session_records import SessionRecordRepository
from kagan.adapters.db.repositories.task import TaskRepository

__all__ = [
    "ClosingAwareSessionFactory",
    "ExecutionRepository",
    "RepoRepository",
    "RepositoryClosing",
    "ScratchRepository",
    "SessionRecordRepository",
    "TaskRepository",
]
