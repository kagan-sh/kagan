"""Execution service interface."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Protocol

if TYPE_CHECKING:
    from collections.abc import Sequence

    from kagan.adapters.db.repositories import TaskRepository
    from kagan.adapters.db.schema import CodingAgentTurn, ExecutionProcess, ExecutionProcessLog
    from kagan.core.models.enums import ExecutionRunReason
    from kagan.services.types import ExecutionId, SessionId


class ExecutionService(Protocol):
    """Service interface for execution operations."""

    async def create_execution(
        self,
        *,
        session_id: str,
        run_reason: ExecutionRunReason,
        executor_action: dict[str, Any] | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> ExecutionProcess:
        """Create a new execution process."""
        ...

    async def update_execution(
        self, execution_id: ExecutionId, **kwargs: object
    ) -> ExecutionProcess | None:
        """Update an execution process."""
        ...

    async def append_log(self, execution_id: ExecutionId, log_line: str) -> ExecutionProcessLog:
        """Append a JSONL log line."""
        ...

    async def get_logs(self, execution_id: ExecutionId) -> ExecutionProcessLog | None:
        """Get execution logs."""
        ...

    async def get_log_entries(self, execution_id: ExecutionId) -> list[ExecutionProcessLog]: ...

    async def get_execution(self, execution_id: ExecutionId) -> ExecutionProcess | None:
        """Get execution by ID."""
        ...

    async def append_agent_turn(
        self,
        execution_id: ExecutionId,
        *,
        agent_session_id: str | None = None,
        prompt: str | None = None,
        summary: str | None = None,
        agent_message_id: str | None = None,
    ) -> CodingAgentTurn:
        """Append a coding agent turn."""
        ...

    async def list_agent_turns(self, execution_id: ExecutionId) -> Sequence[CodingAgentTurn]:
        """List agent turns for an execution."""
        ...

    async def get_latest_agent_turn_for_execution(
        self, execution_id: ExecutionId
    ) -> CodingAgentTurn | None:
        """Get latest agent turn for an execution."""
        ...

    async def get_latest_execution_for_task(self, task_id: str) -> ExecutionProcess | None:
        """Return most recent execution for a task."""
        ...

    async def count_executions_for_task(self, task_id: str) -> int:
        """Return total executions for a task."""
        ...

    async def get_latest_execution_for_session(
        self, session_id: SessionId
    ) -> ExecutionProcess | None:
        """Return most recent execution for a session."""
        ...

    async def get_running_execution_for_session(
        self, session_id: SessionId
    ) -> ExecutionProcess | None:
        """Return running execution for a session."""
        ...


class ExecutionServiceImpl:
    """Execution service backed by TaskRepository."""

    def __init__(self, repo: TaskRepository) -> None:
        self._repo = repo

    async def create_execution(
        self,
        *,
        session_id: str,
        run_reason: ExecutionRunReason,
        executor_action: dict[str, Any] | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> ExecutionProcess:
        return await self._repo.create_execution(
            session_id=session_id,
            run_reason=run_reason,
            executor_action=executor_action,
            metadata=metadata,
        )

    async def update_execution(
        self, execution_id: ExecutionId, **kwargs: object
    ) -> ExecutionProcess | None:
        return await self._repo.update_execution(execution_id, **kwargs)

    async def append_log(self, execution_id: ExecutionId, log_line: str) -> ExecutionProcessLog:
        return await self._repo.append_execution_log(execution_id, log_line)

    async def get_logs(self, execution_id: ExecutionId) -> ExecutionProcessLog | None:
        return await self._repo.get_execution_logs(execution_id)

    async def get_log_entries(self, execution_id: ExecutionId) -> list[ExecutionProcessLog]:
        return await self._repo.get_execution_log_entries(execution_id)

    async def get_execution(self, execution_id: ExecutionId) -> ExecutionProcess | None:
        return await self._repo.get_execution(execution_id)

    async def append_agent_turn(
        self,
        execution_id: ExecutionId,
        *,
        agent_session_id: str | None = None,
        prompt: str | None = None,
        summary: str | None = None,
        agent_message_id: str | None = None,
    ) -> CodingAgentTurn:
        return await self._repo.append_agent_turn(
            execution_id,
            agent_session_id=agent_session_id,
            prompt=prompt,
            summary=summary,
            agent_message_id=agent_message_id,
        )

    async def list_agent_turns(self, execution_id: ExecutionId) -> Sequence[CodingAgentTurn]:
        return await self._repo.list_agent_turns(execution_id)

    async def get_latest_agent_turn_for_execution(
        self, execution_id: ExecutionId
    ) -> CodingAgentTurn | None:
        return await self._repo.get_latest_agent_turn_for_execution(execution_id)

    async def get_latest_execution_for_task(self, task_id: str) -> ExecutionProcess | None:
        return await self._repo.get_latest_execution_for_task(task_id)

    async def count_executions_for_task(self, task_id: str) -> int:
        return await self._repo.count_executions_for_task(task_id)

    async def get_latest_execution_for_session(
        self, session_id: SessionId
    ) -> ExecutionProcess | None:
        return await self._repo.get_latest_execution_for_session(session_id)

    async def get_running_execution_for_session(
        self, session_id: SessionId
    ) -> ExecutionProcess | None:
        return await self._repo.get_running_execution_for_session(session_id)
