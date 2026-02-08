from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING, cast
from unittest.mock import AsyncMock

from kagan.adapters.db.repositories import TaskRepository
from kagan.services.runtime import RuntimeServiceImpl

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.adapters.db.repositories import ExecutionRepository
    from kagan.services.projects import ProjectService


async def test_runtime_state_service_persists_last_active_context(tmp_path: Path) -> None:
    db_path = tmp_path / "runtime.db"
    repo = TaskRepository(db_path)
    await repo.initialize()

    project_service = cast("ProjectService", SimpleNamespace())
    execution_service = cast(
        "ExecutionRepository",
        SimpleNamespace(get_latest_execution_for_task=AsyncMock()),
    )
    service = RuntimeServiceImpl(
        project_service=project_service,
        session_factory=repo.session_factory,
        execution_service=execution_service,
    )

    initial = await service.get_last_active_context()
    assert initial.project_id is None
    assert initial.repo_id is None

    await service.set_last_active_context("proj-1", "repo-1")
    saved = await service.get_last_active_context()
    assert saved.project_id == "proj-1"
    assert saved.repo_id == "repo-1"

    await service.set_last_active_context("proj-2", None)
    updated = await service.get_last_active_context()
    assert updated.project_id == "proj-2"
    assert updated.repo_id is None

    await repo.close()
