from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Protocol

from kagan.core.agents.agent_factory import AgentFactory, create_agent
from kagan.core.services.queued_messages import DEFAULT_QUEUE_LANE, QueuedMessageServiceImpl

from .runner import AutomationEngine

if TYPE_CHECKING:
    from collections.abc import Callable

    from kagan.core.acp import Agent
    from kagan.core.adapters.db.repositories import ExecutionRepository
    from kagan.core.adapters.git.operations import GitOperationsProtocol
    from kagan.core.config import KaganConfig
    from kagan.core.events import EventBus
    from kagan.core.models.enums import NotificationSeverity, TaskStatus
    from kagan.core.services.queued_messages import (
        QueuedMessage,
        QueuedMessageService,
        QueueLane,
        QueueStatus,
    )
    from kagan.core.services.runtime import RuntimeService
    from kagan.core.services.sessions import SessionService
    from kagan.core.services.tasks import TaskService
    from kagan.core.services.types import TaskLike
    from kagan.core.services.workspaces import WorkspaceService


class AutomationService(Protocol):
    """Protocol boundary for AUTO execution lifecycle orchestration."""

    @property
    def merge_lock(self) -> asyncio.Lock: ...

    @property
    def running_tasks(self) -> set[str]: ...

    async def start(self) -> None: ...

    async def stop(self) -> None: ...

    def is_running(self, task_id: str) -> bool: ...

    def is_reviewing(self, task_id: str) -> bool: ...

    def get_running_agent(self, task_id: str) -> Agent | None: ...

    def get_review_agent(self, task_id: str) -> Agent | None: ...

    async def wait_for_running_agent(
        self,
        task_id: str,
        *,
        timeout: float = 2.0,
        interval: float = 0.05,
    ) -> Agent | None: ...

    def get_execution_id(self, task_id: str) -> str | None: ...

    def get_run_count(self, task_id: str) -> int: ...

    async def queue_message(
        self,
        session_id: str,
        content: str,
        *,
        lane: QueueLane = DEFAULT_QUEUE_LANE,
        author: str | None = None,
        metadata: dict[str, object] | None = None,
    ) -> QueuedMessage: ...

    async def cancel_queued(
        self, session_id: str, *, lane: QueueLane = DEFAULT_QUEUE_LANE
    ) -> None: ...

    async def get_status(
        self, session_id: str, *, lane: QueueLane = DEFAULT_QUEUE_LANE
    ) -> QueueStatus: ...

    async def take_queued(
        self, session_id: str, *, lane: QueueLane = DEFAULT_QUEUE_LANE
    ) -> QueuedMessage | None: ...

    async def take_all_queued(
        self, session_id: str, *, lane: QueueLane = DEFAULT_QUEUE_LANE
    ) -> list[QueuedMessage]: ...

    async def get_queued(
        self, session_id: str, *, lane: QueueLane = DEFAULT_QUEUE_LANE
    ) -> list[QueuedMessage]: ...

    async def remove_message(
        self, session_id: str, index: int, *, lane: QueueLane = DEFAULT_QUEUE_LANE
    ) -> bool: ...

    async def spawn_for_task(self, task: TaskLike) -> bool: ...

    async def stop_task(self, task_id: str) -> bool: ...


class AutomationServiceImpl:
    """Thin wrapper over AutomationEngine for AUTO task orchestration."""

    def __init__(
        self,
        task_service: TaskService,
        workspace_service: WorkspaceService,
        config: KaganConfig,
        runtime_service: RuntimeService,
        session_service: SessionService | None = None,
        execution_service: ExecutionRepository | None = None,
        on_task_changed: Callable[[], None] | None = None,
        on_error: Callable[[str, str], None] | None = None,
        notifier: Callable[[str, str, NotificationSeverity], None] | None = None,
        agent_factory: AgentFactory = create_agent,
        event_bus: EventBus | None = None,
        queued_message_service: QueuedMessageService | None = None,
        git_adapter: GitOperationsProtocol | None = None,
    ) -> None:
        self._merge_lock = asyncio.Lock()
        self._queued = queued_message_service or QueuedMessageServiceImpl()
        self._engine = AutomationEngine(
            task_service=task_service,
            workspace_service=workspace_service,
            config=config,
            runtime_service=runtime_service,
            session_service=session_service,
            execution_service=execution_service,
            on_task_changed=on_task_changed,
            on_error=on_error,
            notifier=notifier,
            agent_factory=agent_factory,
            event_bus=event_bus,
            queued_message_service=self._queued,
            git_adapter=git_adapter,
        )

    @property
    def merge_lock(self) -> asyncio.Lock:
        """Lock for serializing merge operations."""
        return self._merge_lock

    @property
    def running_tasks(self) -> set[str]:
        return self._engine.running_tasks

    async def start(self) -> None:
        await self._engine.start()

    async def stop(self) -> None:
        await self._engine.stop()

    def is_running(self, task_id: str) -> bool:
        return self._engine.is_running(task_id)

    def is_reviewing(self, task_id: str) -> bool:
        return self._engine.is_reviewing(task_id)

    def get_running_agent(self, task_id: str) -> Agent | None:
        return self._engine.get_running_agent(task_id)

    def get_review_agent(self, task_id: str) -> Agent | None:
        return self._engine.get_review_agent(task_id)

    async def wait_for_running_agent(
        self,
        task_id: str,
        *,
        timeout: float = 2.0,
        interval: float = 0.05,
    ) -> Agent | None:
        return await self._engine.wait_for_running_agent(
            task_id,
            timeout=timeout,
            interval=interval,
        )

    def get_execution_id(self, task_id: str) -> str | None:
        return self._engine.get_execution_id(task_id)

    def get_run_count(self, task_id: str) -> int:
        return self._engine.get_run_count(task_id)

    async def queue_message(
        self,
        session_id: str,
        content: str,
        *,
        lane: QueueLane = DEFAULT_QUEUE_LANE,
        author: str | None = None,
        metadata: dict[str, object] | None = None,
    ) -> QueuedMessage:
        return await self._queued.queue_message(
            session_id,
            content,
            lane=lane,
            author=author,
            metadata=metadata,
        )

    async def cancel_queued(self, session_id: str, *, lane: QueueLane = DEFAULT_QUEUE_LANE) -> None:
        await self._queued.cancel_queued(session_id, lane=lane)

    async def get_status(
        self, session_id: str, *, lane: QueueLane = DEFAULT_QUEUE_LANE
    ) -> QueueStatus:
        return await self._queued.get_status(session_id, lane=lane)

    async def take_queued(
        self, session_id: str, *, lane: QueueLane = DEFAULT_QUEUE_LANE
    ) -> QueuedMessage | None:
        return await self._queued.take_queued(session_id, lane=lane)

    async def take_all_queued(
        self, session_id: str, *, lane: QueueLane = DEFAULT_QUEUE_LANE
    ) -> list[QueuedMessage]:
        return await self._queued.take_all_queued(session_id, lane=lane)

    async def get_queued(
        self, session_id: str, *, lane: QueueLane = DEFAULT_QUEUE_LANE
    ) -> list[QueuedMessage]:
        return await self._queued.get_queued(session_id, lane=lane)

    async def remove_message(
        self, session_id: str, index: int, *, lane: QueueLane = DEFAULT_QUEUE_LANE
    ) -> bool:
        return await self._queued.remove_message(session_id, index, lane=lane)

    async def handle_status_change(
        self,
        task_id: str,
        old_status: TaskStatus | None,
        new_status: TaskStatus | None,
    ) -> None:
        await self._engine.handle_status_change(task_id, old_status, new_status)

    async def spawn_for_task(self, task: TaskLike) -> bool:
        return await self._engine.spawn_for_task(task)

    async def stop_task(self, task_id: str) -> bool:
        return await self._engine.stop_task(task_id)
