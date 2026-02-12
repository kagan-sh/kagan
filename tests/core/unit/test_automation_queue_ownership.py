from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING, cast

from tests.helpers.mocks import create_test_config

from kagan.core.services.automation import AutomationServiceImpl

if TYPE_CHECKING:
    from kagan.core.agents.agent_factory import AgentFactory
    from kagan.core.services.runtime import RuntimeService
    from kagan.core.services.tasks import TaskService
    from kagan.core.services.workspaces import WorkspaceService


def _runtime_stub() -> RuntimeService:
    return cast(
        "RuntimeService",
        SimpleNamespace(
            get=lambda _task_id: None,
            running_tasks=lambda: set(),
            mark_started=lambda _task_id: None,
            mark_ended=lambda _task_id: None,
            set_execution=lambda _task_id, _execution_id, _run_count: None,
            attach_running_agent=lambda _task_id, _agent: None,
            attach_review_agent=lambda _task_id, _agent: None,
            clear_review_agent=lambda _task_id: None,
            mark_blocked=lambda _task_id, **_kwargs: None,
            clear_blocked=lambda _task_id: None,
        ),
    )


async def test_automation_service_owns_queue_operations() -> None:
    service = AutomationServiceImpl(
        task_service=cast("TaskService", SimpleNamespace()),
        workspace_service=cast("WorkspaceService", SimpleNamespace()),
        config=create_test_config(),
        runtime_service=_runtime_stub(),
        agent_factory=cast("AgentFactory", lambda *_args, **_kwargs: SimpleNamespace()),
    )

    await service.queue_message("task-1", "impl-message", lane="implementation")
    await service.queue_message("task-1", "review-message", lane="review")

    impl = await service.take_queued("task-1", lane="implementation")
    assert impl is not None
    assert impl.content == "impl-message"

    status = await service.get_status("task-1", lane="review")
    assert status.has_queued is True

    review = await service.take_queued("task-1", lane="review")
    assert review is not None
    assert review.content == "review-message"
