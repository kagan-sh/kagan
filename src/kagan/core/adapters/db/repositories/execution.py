"""Execution repository behavior."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Any

from sqlalchemy import func
from sqlmodel import col, select

from kagan.core.adapters.db.schema import (
    CodingAgentTurn,
    ExecutionProcess,
    ExecutionProcessLog,
    ExecutionProcessRepoState,
    Session,
    Workspace,
)
from kagan.core.models.enums import ExecutionRunReason, ExecutionStatus
from kagan.core.time import utc_now

if TYPE_CHECKING:
    from collections.abc import Sequence

    from sqlalchemy.ext.asyncio import AsyncSession

    from kagan.core.adapters.db.repositories.base import ClosingAwareSessionFactory


class ExecutionRepository:
    """Execution-process repository."""

    def __init__(self, session_factory: ClosingAwareSessionFactory) -> None:
        self._session_factory = session_factory
        self._lock = asyncio.Lock()

    def _get_session(self) -> AsyncSession:
        return self._session_factory()

    async def create_execution(
        self,
        *,
        session_id: str,
        run_reason: ExecutionRunReason,
        executor_action: dict[str, Any] | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> ExecutionProcess:
        """Create a new execution process."""
        async with self._lock:
            async with self._get_session() as session:
                execution = ExecutionProcess(
                    session_id=session_id,
                    run_reason=run_reason,
                    executor_action=executor_action or {},
                    status=ExecutionStatus.RUNNING,
                    metadata_=metadata or {},
                    started_at=utc_now(),
                    created_at=utc_now(),
                    updated_at=utc_now(),
                )
                session.add(execution)
                await session.commit()
                await session.refresh(execution)
                return execution

    async def update_execution(self, execution_id: str, **kwargs: Any) -> ExecutionProcess | None:
        """Update an execution process."""
        async with self._lock:
            async with self._get_session() as session:
                execution = await session.get(ExecutionProcess, execution_id)
                if not execution:
                    return None

                update_data = {k: v for k, v in kwargs.items() if v is not None}
                if "metadata" in update_data and "metadata_" not in update_data:
                    update_data["metadata_"] = update_data.pop("metadata")
                if update_data:
                    execution.sqlmodel_update(update_data)
                execution.updated_at = utc_now()

                session.add(execution)
                await session.commit()
                await session.refresh(execution)
                return execution

    async def append_execution_log(self, execution_id: str, log_line: str) -> ExecutionProcessLog:
        """Append a JSONL log line for an execution."""
        async with self._lock:
            async with self._get_session() as session:
                log_entry = ExecutionProcessLog(
                    execution_process_id=execution_id,
                    logs=log_line,
                    byte_size=len(log_line.encode("utf-8")),
                    inserted_at=utc_now(),
                )
                session.add(log_entry)
                await session.commit()
                await session.refresh(log_entry)
                return log_entry

    async def get_execution_logs(self, execution_id: str) -> ExecutionProcessLog | None:
        """Return aggregated execution logs for an execution."""
        async with self._get_session() as session:
            result = await session.execute(
                select(ExecutionProcessLog)
                .where(ExecutionProcessLog.execution_process_id == execution_id)
                .order_by(
                    col(ExecutionProcessLog.inserted_at).asc(),
                    col(ExecutionProcessLog.id).asc(),
                )
            )
            entries = result.scalars().all()
            if not entries:
                return None

            combined_logs = "\n".join(entry.logs for entry in entries if entry.logs)
            total_bytes = sum(entry.byte_size for entry in entries)
            latest = entries[-1]
            return ExecutionProcessLog(
                id=latest.id,
                execution_process_id=execution_id,
                logs=combined_logs,
                byte_size=total_bytes,
                inserted_at=latest.inserted_at,
            )

    async def get_execution_log_entries(self, execution_id: str) -> list[ExecutionProcessLog]:
        """Return ordered execution log entries for an execution."""
        async with self._get_session() as session:
            result = await session.execute(
                select(ExecutionProcessLog)
                .where(ExecutionProcessLog.execution_process_id == execution_id)
                .order_by(
                    col(ExecutionProcessLog.inserted_at).asc(),
                    col(ExecutionProcessLog.id).asc(),
                )
            )
            return list(result.scalars().all())

    async def get_execution(self, execution_id: str) -> ExecutionProcess | None:
        """Return execution record by ID."""
        async with self._get_session() as session:
            return await session.get(ExecutionProcess, execution_id)

    async def append_agent_turn(
        self,
        execution_id: str,
        *,
        agent_session_id: str | None = None,
        prompt: str | None = None,
        summary: str | None = None,
        agent_message_id: str | None = None,
    ) -> CodingAgentTurn:
        """Append a coding agent turn."""
        async with self._lock:
            async with self._get_session() as session:
                turn = CodingAgentTurn(
                    execution_process_id=execution_id,
                    agent_session_id=agent_session_id,
                    prompt=prompt,
                    summary=summary,
                    agent_message_id=agent_message_id,
                    seen=False,
                    created_at=utc_now(),
                    updated_at=utc_now(),
                )
                session.add(turn)
                await session.commit()
                await session.refresh(turn)
                return turn

    async def list_agent_turns(self, execution_id: str) -> Sequence[CodingAgentTurn]:
        """List coding agent turns for an execution."""
        async with self._get_session() as session:
            result = await session.execute(
                select(CodingAgentTurn)
                .where(CodingAgentTurn.execution_process_id == execution_id)
                .order_by(col(CodingAgentTurn.created_at).asc(), col(CodingAgentTurn.id).asc())
            )
            return result.scalars().all()

    async def get_latest_agent_turn_for_execution(
        self, execution_id: str
    ) -> CodingAgentTurn | None:
        """Return the latest coding agent turn for an execution."""
        async with self._get_session() as session:
            result = await session.execute(
                select(CodingAgentTurn)
                .where(CodingAgentTurn.execution_process_id == execution_id)
                .order_by(col(CodingAgentTurn.created_at).desc(), col(CodingAgentTurn.id).desc())
                .limit(1)
            )
            return result.scalars().first()

    async def add_execution_repo_state(
        self,
        execution_id: str,
        repo_id: str,
        *,
        before_head_commit: str | None = None,
        after_head_commit: str | None = None,
        merge_commit: str | None = None,
    ) -> ExecutionProcessRepoState:
        """Persist per-repo state for an execution."""
        async with self._lock:
            async with self._get_session() as session:
                state = ExecutionProcessRepoState(
                    execution_process_id=execution_id,
                    repo_id=repo_id,
                    before_head_commit=before_head_commit,
                    after_head_commit=after_head_commit,
                    merge_commit=merge_commit,
                    created_at=utc_now(),
                    updated_at=utc_now(),
                )
                session.add(state)
                await session.commit()
                await session.refresh(state)
                return state

    async def get_latest_execution_for_task(self, task_id: str) -> ExecutionProcess | None:
        """Return most recent execution for a task."""
        async with self._get_session() as session:
            result = await session.execute(
                select(ExecutionProcess)
                .join(Session, col(ExecutionProcess.session_id) == col(Session.id))
                .join(Workspace, col(Session.workspace_id) == col(Workspace.id))
                .where(Workspace.task_id == task_id)
                .order_by(col(ExecutionProcess.created_at).desc())
                .limit(1)
            )
            return result.scalars().first()

    async def list_executions_for_task(
        self, task_id: str, *, limit: int = 5
    ) -> list[ExecutionProcess]:
        """Return most recent executions for a task."""
        async with self._get_session() as session:
            result = await session.execute(
                select(ExecutionProcess)
                .join(Session, col(ExecutionProcess.session_id) == col(Session.id))
                .join(Workspace, col(Session.workspace_id) == col(Workspace.id))
                .where(Workspace.task_id == task_id)
                .order_by(col(ExecutionProcess.created_at).desc())
                .limit(limit)
            )
            return list(result.scalars().all())

    async def get_latest_execution_for_session(self, session_id: str) -> ExecutionProcess | None:
        """Return most recent execution for a session."""
        async with self._get_session() as session:
            result = await session.execute(
                select(ExecutionProcess)
                .where(ExecutionProcess.session_id == session_id)
                .order_by(col(ExecutionProcess.created_at).desc())
                .limit(1)
            )
            return result.scalars().first()

    async def get_running_execution_for_session(self, session_id: str) -> ExecutionProcess | None:
        """Return running execution for a session, if any."""
        async with self._get_session() as session:
            result = await session.execute(
                select(ExecutionProcess)
                .where(
                    ExecutionProcess.session_id == session_id,
                    ExecutionProcess.status == ExecutionStatus.RUNNING,
                )
                .order_by(col(ExecutionProcess.created_at).desc())
                .limit(1)
            )
            return result.scalars().first()

    async def get_latest_running_executions_for_tasks(
        self, task_ids: Sequence[str]
    ) -> dict[str, str]:
        """Return latest running execution IDs keyed by task ID."""
        if not task_ids:
            return {}

        unique_task_ids = tuple(dict.fromkeys(task_ids))
        async with self._get_session() as session:
            result = await session.execute(
                select(
                    Workspace.task_id,
                    ExecutionProcess.id,
                    ExecutionProcess.created_at,
                )
                .join(Session, col(ExecutionProcess.session_id) == col(Session.id))
                .join(Workspace, col(Session.workspace_id) == col(Workspace.id))
                .where(
                    Workspace.task_id.in_(unique_task_ids),
                    ExecutionProcess.status == ExecutionStatus.RUNNING,
                )
                .order_by(col(ExecutionProcess.created_at).desc())
            )
            rows = result.all()

        latest_by_task: dict[str, str] = {}
        for task_id, execution_id, _created_at in rows:
            if task_id not in latest_by_task:
                latest_by_task[task_id] = execution_id
        return latest_by_task

    async def count_executions_for_task(self, task_id: str) -> int:
        """Return total executions for a task."""
        async with self._get_session() as session:
            result = await session.execute(
                select(func.count())
                .select_from(ExecutionProcess)
                .join(Session, col(ExecutionProcess.session_id) == col(Session.id))
                .join(Workspace, col(Session.workspace_id) == col(Workspace.id))
                .where(Workspace.task_id == task_id)
            )
            return int(result.scalar_one() or 0)
