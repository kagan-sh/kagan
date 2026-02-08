from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING, cast
from unittest.mock import AsyncMock

from kagan.adapters.db.repositories import TaskRepository
from kagan.adapters.db.schema import Project
from kagan.services.runtime import RuntimeContextState, RuntimeServiceImpl, RuntimeSessionEvent

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.adapters.db.repositories import ExecutionRepository
    from kagan.services.projects import ProjectService


def _mock_execution_service() -> ExecutionRepository:
    return cast(
        "ExecutionRepository",
        SimpleNamespace(
            get_latest_execution_for_task=AsyncMock(return_value=None),
            get_execution_log_entries=AsyncMock(return_value=[]),
            update_execution=AsyncMock(return_value=None),
        ),
    )


async def test_runtime_session_dispatch_reduces_and_persists(tmp_path: Path) -> None:
    repo = TaskRepository(tmp_path / "runtime.db")
    await repo.initialize()

    project_service = cast("ProjectService", SimpleNamespace())
    service = RuntimeServiceImpl(
        project_service=project_service,
        session_factory=repo.session_factory,
        execution_service=_mock_execution_service(),
    )

    state = await service.dispatch(RuntimeSessionEvent.PROJECT_SELECTED, project_id="proj-1")
    assert state.project_id == "proj-1"
    assert state.repo_id is None

    state = await service.dispatch(RuntimeSessionEvent.REPO_SELECTED, repo_id="repo-1")
    assert state.project_id == "proj-1"
    assert state.repo_id == "repo-1"

    state = await service.dispatch(RuntimeSessionEvent.REPO_CLEARED)
    assert state.project_id == "proj-1"
    assert state.repo_id is None

    persisted = await service.get_last_active_context()
    assert persisted == RuntimeContextState(project_id="proj-1", repo_id=None)

    await repo.close()


async def test_decide_startup_prefers_persisted_project(tmp_path: Path) -> None:
    repo = TaskRepository(tmp_path / "runtime.db")
    await repo.initialize()

    persisted_project = Project(id="proj-persisted", name="Persisted")
    project_service_ns = SimpleNamespace(
        get_project=AsyncMock(return_value=persisted_project),
        find_project_by_repo_path=AsyncMock(return_value=None),
    )
    project_service = cast(
        "ProjectService",
        project_service_ns,
    )
    service = RuntimeServiceImpl(
        project_service=project_service,
        session_factory=repo.session_factory,
        execution_service=_mock_execution_service(),
    )
    await service.set_last_active_context("proj-persisted", "repo-persisted")

    decision = await service.decide_startup(tmp_path)

    assert decision.project_id == "proj-persisted"
    assert decision.preferred_repo_id == "repo-persisted"
    project_service_ns.find_project_by_repo_path.assert_not_awaited()

    await repo.close()


async def test_decide_startup_clears_stale_persisted_then_uses_cwd(tmp_path: Path) -> None:
    repo = TaskRepository(tmp_path / "runtime.db")
    await repo.initialize()

    cwd_project = Project(id="proj-cwd", name="CWD")
    project_service = cast(
        "ProjectService",
        SimpleNamespace(
            get_project=AsyncMock(return_value=None),
            find_project_by_repo_path=AsyncMock(return_value=cwd_project),
        ),
    )
    service = RuntimeServiceImpl(
        project_service=project_service,
        session_factory=repo.session_factory,
        execution_service=_mock_execution_service(),
    )
    await service.set_last_active_context("missing", "repo-old")

    decision = await service.decide_startup(tmp_path)

    assert decision.project_id == "proj-cwd"
    assert decision.preferred_path == tmp_path
    persisted = await service.get_last_active_context()
    assert persisted == RuntimeContextState(project_id=None, repo_id=None)

    await repo.close()


async def test_decide_startup_falls_back_to_welcome(tmp_path: Path, monkeypatch) -> None:
    repo = TaskRepository(tmp_path / "runtime.db")
    await repo.initialize()

    project_service = cast(
        "ProjectService",
        SimpleNamespace(
            get_project=AsyncMock(return_value=None),
            find_project_by_repo_path=AsyncMock(return_value=None),
        ),
    )
    service = RuntimeServiceImpl(
        project_service=project_service,
        session_factory=repo.session_factory,
        execution_service=_mock_execution_service(),
    )
    monkeypatch.setattr(
        "kagan.services.runtime.has_git_repo",
        AsyncMock(return_value=True),
    )

    decision = await service.decide_startup(tmp_path)

    assert decision.project_id is None
    assert decision.suggest_cwd is True
    assert decision.cwd_path == str(tmp_path)

    await repo.close()
