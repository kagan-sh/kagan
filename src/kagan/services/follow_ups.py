"""Follow-up execution service."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Protocol

from kagan.core.models.enums import ExecutionRunReason

if TYPE_CHECKING:
    from kagan.adapters.db.schema import CodingAgentTurn, ExecutionProcess
    from kagan.services.executions import ExecutionService
    from kagan.services.queued_messages import QueuedMessage, QueuedMessageService
    from kagan.services.types import SessionId


class FollowUpService(Protocol):
    """Service interface for follow-up executions."""

    async def start_follow_up(
        self,
        session_id: SessionId,
        *,
        reason: ExecutionRunReason = ExecutionRunReason.CODINGAGENT,
    ) -> ExecutionProcess:
        """Start a follow-up execution for a session."""
        ...

    async def resume_follow_up(
        self,
        session_id: SessionId,
        *,
        reason: ExecutionRunReason = ExecutionRunReason.CODINGAGENT,
    ) -> ExecutionProcess | None:
        """Return running execution if present, otherwise start a follow-up."""
        ...


class FollowUpServiceImpl:
    """Service for starting follow-up executions based on queued messages."""

    def __init__(
        self,
        execution_service: ExecutionService,
        queued_messages: QueuedMessageService,
    ) -> None:
        self._executions = execution_service
        self._queued = queued_messages

    async def start_follow_up(
        self,
        session_id: SessionId,
        *,
        reason: ExecutionRunReason = ExecutionRunReason.CODINGAGENT,
    ) -> ExecutionProcess:
        latest_execution = await self._executions.get_latest_execution_for_session(session_id)
        if latest_execution is None:
            raise ValueError(f"No execution found for session {session_id}")

        latest_turn = await self._executions.get_latest_agent_turn_for_execution(
            latest_execution.id
        )
        if latest_turn is None:
            raise ValueError(f"No agent turns found for execution {latest_execution.id}")

        queued_message = await self._queued.take_queued(session_id)
        prompt = self._build_follow_up_prompt(latest_turn, queued_message)

        execution = await self._executions.create_execution(
            session_id=session_id,
            run_reason=reason,
            executor_action={
                "follow_up": True,
                "source_execution_id": latest_execution.id,
            },
        )
        await self._executions.append_log(execution.id, self._serialize_prompt(prompt))
        return execution

    async def resume_follow_up(
        self,
        session_id: SessionId,
        *,
        reason: ExecutionRunReason = ExecutionRunReason.CODINGAGENT,
    ) -> ExecutionProcess | None:
        running = await self._executions.get_running_execution_for_session(session_id)
        if running is not None:
            return running
        return await self.start_follow_up(session_id, reason=reason)

    def _build_follow_up_prompt(
        self,
        latest_turn: CodingAgentTurn,
        queued_message: QueuedMessage | None,
    ) -> str:
        lines = [
            "Follow-up requested for the previous execution.",
            "",
        ]
        if latest_turn.summary:
            lines.append("Previous summary:")
            lines.append(latest_turn.summary)
            lines.append("")
        if latest_turn.prompt:
            lines.append("Previous prompt:")
            lines.append(latest_turn.prompt)
            lines.append("")
        if queued_message is not None:
            lines.append("User follow-up message:")
            lines.append(queued_message.content)
        else:
            lines.append("No queued follow-up message was provided.")
        return "\n".join(lines).strip()

    def _serialize_prompt(self, prompt: str) -> str:
        return json.dumps(
            {
                "messages": [
                    {
                        "type": "response",
                        "content": prompt,
                    }
                ]
            }
        )
