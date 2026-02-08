from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING, cast
from unittest.mock import AsyncMock, MagicMock

from kagan.adapters.db.repositories import TaskRepository
from kagan.services.runtime import RuntimeServiceImpl, RuntimeTaskPhase

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.adapters.db.repositories import ExecutionRepository
    from kagan.services.projects import ProjectService


async def _make_runtime_service(tmp_path: Path):
    repo = TaskRepository(tmp_path / "runtime-view.db")
    await repo.initialize()
    project_service = cast("ProjectService", SimpleNamespace())
    execution_service = cast(
        "ExecutionRepository",
        SimpleNamespace(
            get_latest_execution_for_task=AsyncMock(return_value=None),
            get_execution_log_entries=AsyncMock(return_value=[]),
            update_execution=AsyncMock(return_value=None),
        ),
    )
    service = RuntimeServiceImpl(
        project_service=project_service,
        session_factory=repo.session_factory,
        execution_service=execution_service,
    )
    return service, repo


async def test_runtime_task_view_tracks_running_and_reviewing_agents(tmp_path: Path) -> None:
    service, repo = await _make_runtime_service(tmp_path)
    impl_agent = MagicMock()
    review_agent = MagicMock()

    service.mark_started("task-1")
    service.set_execution("task-1", "exec-1", 2)
    service.attach_running_agent("task-1", impl_agent)
    service.attach_review_agent("task-1", review_agent)

    view = service.get("task-1")
    assert view is not None
    assert view.phase == RuntimeTaskPhase.REVIEWING
    assert view.execution_id == "exec-1"
    assert view.run_count == 2
    assert view.running_agent is impl_agent
    assert view.review_agent is review_agent

    service.clear_review_agent("task-1")
    view = service.get("task-1")
    assert view is not None
    assert view.phase == RuntimeTaskPhase.RUNNING
    assert view.review_agent is None
    await repo.close()


async def test_runtime_task_view_removes_task_on_end(tmp_path: Path) -> None:
    service, repo = await _make_runtime_service(tmp_path)
    service.mark_started("task-1")
    assert service.running_tasks() == {"task-1"}

    service.mark_ended("task-1")
    assert service.get("task-1") is None
    assert service.running_tasks() == set()
    await repo.close()
