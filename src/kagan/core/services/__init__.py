"""Service layer interfaces."""

from kagan.core.adapters.db.repositories import ExecutionRepository
from kagan.core.services.automation import AutomationService, AutomationServiceImpl
from kagan.core.services.diffs import DiffService, DiffServiceImpl
from kagan.core.services.jobs import JobService, JobServiceImpl
from kagan.core.services.merges import MergeService, MergeServiceImpl
from kagan.core.services.projects import ProjectService, ProjectServiceImpl
from kagan.core.services.queued_messages import QueuedMessageService, QueuedMessageServiceImpl
from kagan.core.services.reviews import ReviewService, ReviewServiceImpl
from kagan.core.services.runtime import (
    AutoOutputMode,
    AutoOutputReadiness,
    AutoOutputRecoveryResult,
    RuntimeContextState,
    RuntimeService,
    RuntimeServiceImpl,
    RuntimeSessionEvent,
    RuntimeTaskPhase,
    RuntimeTaskView,
    StartupSessionDecision,
)
from kagan.core.services.sessions import SessionService, SessionServiceImpl
from kagan.core.services.tasks import TaskService, TaskServiceImpl
from kagan.core.services.workspaces import WorkspaceService, WorkspaceServiceImpl

__all__ = [
    "AutoOutputMode",
    "AutoOutputReadiness",
    "AutoOutputRecoveryResult",
    "AutomationService",
    "AutomationServiceImpl",
    "DiffService",
    "DiffServiceImpl",
    "ExecutionRepository",
    "JobService",
    "JobServiceImpl",
    "MergeService",
    "MergeServiceImpl",
    "ProjectService",
    "ProjectServiceImpl",
    "QueuedMessageService",
    "QueuedMessageServiceImpl",
    "ReviewService",
    "ReviewServiceImpl",
    "RuntimeContextState",
    "RuntimeService",
    "RuntimeServiceImpl",
    "RuntimeSessionEvent",
    "RuntimeTaskPhase",
    "RuntimeTaskView",
    "SessionService",
    "SessionServiceImpl",
    "StartupSessionDecision",
    "TaskService",
    "TaskServiceImpl",
    "WorkspaceService",
    "WorkspaceServiceImpl",
]
