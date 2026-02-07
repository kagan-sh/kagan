"""Execution service interface."""

from __future__ import annotations

from datetime import datetime
from typing import TYPE_CHECKING, Protocol
from uuid import uuid4

from kagan.core.models.enums import ExecutionStatus

if TYPE_CHECKING:
    from kagan.core.models.entities import ExecutionProcess
    from kagan.services.types import ExecutionId, TaskId, WorkspaceId


class ExecutionService(Protocol):
    """Service interface for execution operations."""

    async def request(
        self,
        task_id: TaskId,
        *,
        workspace_id: WorkspaceId | None = None,
        executor: str,
        command: list[str] | None = None,
    ) -> ExecutionProcess:
        """Request a new execution."""

    async def cancel(self, execution_id: ExecutionId, *, reason: str | None = None) -> None:
        """Cancel an execution."""

    async def get_execution(self, execution_id: ExecutionId) -> ExecutionProcess | None:
        """Return an execution by ID."""

    async def list_executions(
        self,
        *,
        task_id: TaskId | None = None,
        workspace_id: WorkspaceId | None = None,
    ) -> list[ExecutionProcess]:
        """List executions filtered by task or workspace."""


class ExecutionServiceImpl:
    """In-memory execution service (placeholder implementation)."""

    def __init__(self) -> None:
        self._executions: dict[str, ExecutionProcess] = {}

    async def request(
        self,
        task_id: TaskId,
        *,
        workspace_id: WorkspaceId | None = None,
        executor: str,
        command: list[str] | None = None,
    ) -> ExecutionProcess:
        from kagan.core.models.entities import ExecutionProcess

        del command
        execution_id = uuid4().hex[:8]
        execution = ExecutionProcess(
            id=execution_id,
            task_id=task_id,
            workspace_id=workspace_id,
            session_id=None,
            status=ExecutionStatus.PENDING,
            executor=executor,
            started_at=datetime.now(),
            finished_at=None,
            exit_code=None,
            error=None,
            metadata={},
        )
        self._executions[execution_id] = execution
        return execution

    async def cancel(self, execution_id: ExecutionId, *, reason: str | None = None) -> None:
        del reason
        execution = self._executions.get(execution_id)
        if execution:
            execution.status = ExecutionStatus.CANCELED
            execution.finished_at = datetime.now()

    async def get_execution(self, execution_id: ExecutionId) -> ExecutionProcess | None:
        return self._executions.get(execution_id)

    async def list_executions(
        self,
        *,
        task_id: TaskId | None = None,
        workspace_id: WorkspaceId | None = None,
    ) -> list[ExecutionProcess]:
        executions = list(self._executions.values())
        if task_id:
            executions = [e for e in executions if e.task_id == task_id]
        if workspace_id:
            executions = [e for e in executions if e.workspace_id == workspace_id]
        return executions
