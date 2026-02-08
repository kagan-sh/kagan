from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Protocol

from kagan.agents.agent_factory import AgentFactory, create_agent

from .runner import AutomationEngine

if TYPE_CHECKING:
    from collections.abc import Callable

    from kagan.acp import Agent
    from kagan.adapters.db.repositories import ExecutionRepository
    from kagan.adapters.git.operations import GitOperationsProtocol
    from kagan.config import KaganConfig
    from kagan.core.events import EventBus
    from kagan.core.models.enums import NotificationSeverity, TaskStatus
    from kagan.services.queued_messages import QueuedMessageService
    from kagan.services.runtime import RuntimeService
    from kagan.services.sessions import SessionService
    from kagan.services.tasks import TaskService
    from kagan.services.types import TaskLike
    from kagan.services.workspaces import WorkspaceService


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

    async def spawn_for_task(self, task: TaskLike) -> bool: ...

    async def stop_task(self, task_id: str) -> bool: ...


class AutomationServiceImpl:
    """Thin facade over AutomationEngine for AUTO task orchestration."""

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
            queued_message_service=queued_message_service,
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
