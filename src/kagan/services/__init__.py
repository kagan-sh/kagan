"""Service layer interfaces."""

from kagan.services.automation import AutomationService, AutomationServiceImpl
from kagan.services.executions import ExecutionService, ExecutionServiceImpl
from kagan.services.merges import MergeService, MergeServiceImpl
from kagan.services.sessions import SessionService, SessionServiceImpl
from kagan.services.tasks import TaskService, TaskServiceImpl
from kagan.services.workspaces import WorkspaceService, WorkspaceServiceImpl

__all__ = [
    "AutomationService",
    "AutomationServiceImpl",
    "ExecutionService",
    "ExecutionServiceImpl",
    "MergeService",
    "MergeServiceImpl",
    "SessionService",
    "SessionServiceImpl",
    "TaskService",
    "TaskServiceImpl",
    "WorkspaceService",
    "WorkspaceServiceImpl",
]
