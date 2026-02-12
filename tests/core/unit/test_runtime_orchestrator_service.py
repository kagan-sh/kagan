from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, cast
from unittest.mock import AsyncMock, MagicMock

import pytest

from kagan.core.adapters.db.repositories import TaskRepository
from kagan.core.adapters.db.schema import Project
from kagan.core.models.enums import ExecutionStatus, TaskType
from kagan.core.runtime_helpers import (
    RuntimeSnapshotSource,
    empty_runtime_snapshot,
    runtime_snapshot_for_task,
    serialize_runtime_view,
)
from kagan.core.services.runtime import (
    AutoOutputMode,
    RuntimeContextState,
    RuntimeServiceImpl,
    RuntimeSessionEvent,
    RuntimeTaskPhase,
    RuntimeTaskView,
)

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.core.adapters.db.repositories import ExecutionRepository
    from kagan.core.services.projects import ProjectService
    from kagan.core.services.types import TaskLike


@dataclass(slots=True)
class _Task:
    id: str
    task_type: TaskType


async def _make_service(
    tmp_path: Path,
    *,
    latest_execution: object | None = None,
    log_entries: list[SimpleNamespace] | None = None,
    execution_by_id: dict[str, object | None] | None = None,
    spawned: bool = False,
    runtime_view: RuntimeTaskView | None = None,
) -> tuple[RuntimeServiceImpl, SimpleNamespace, SimpleNamespace, TaskRepository]:
    async def _get_execution(execution_id: str) -> object | None:
        if execution_by_id is None:
            return None
        return execution_by_id.get(execution_id)

    automation = SimpleNamespace(
        wait_for_running_agent=AsyncMock(return_value=None),
        spawn_for_task=AsyncMock(return_value=spawned),
    )
    executions = cast(
        "ExecutionRepository",
        SimpleNamespace(
            get_latest_execution_for_task=AsyncMock(return_value=latest_execution),
            get_execution_log_entries=AsyncMock(return_value=log_entries or []),
            get_execution=AsyncMock(side_effect=_get_execution),
            update_execution=AsyncMock(return_value=None),
        ),
    )

    repo = TaskRepository(tmp_path / "runtime-orchestrator.db")
    await repo.initialize()
    service = RuntimeServiceImpl(
        project_service=cast("ProjectService", SimpleNamespace()),
        session_factory=repo.session_factory,
        execution_service=executions,
        automation_resolver=lambda: cast("Any", automation),
    )

    if runtime_view is not None and runtime_view.is_running:
        service.mark_started(runtime_view.task_id)
        service.set_execution(
            runtime_view.task_id,
            runtime_view.execution_id,
            runtime_view.run_count,
        )
        if runtime_view.running_agent is not None:
            service.attach_running_agent(runtime_view.task_id, runtime_view.running_agent)
        if runtime_view.review_agent is not None:
            service.attach_review_agent(runtime_view.task_id, runtime_view.review_agent)
        if runtime_view.phase is RuntimeTaskPhase.REVIEWING and runtime_view.review_agent is None:
            service.attach_review_agent(runtime_view.task_id, cast("Any", object()))
    return service, automation, cast("Any", executions), repo


def _mock_execution_service() -> ExecutionRepository:
    return cast(
        "ExecutionRepository",
        SimpleNamespace(
            get_latest_execution_for_task=AsyncMock(return_value=None),
            get_execution_log_entries=AsyncMock(return_value=[]),
            update_execution=AsyncMock(return_value=None),
        ),
    )


async def _make_runtime_service(
    tmp_path: Path,
    *,
    get_project_result: Project | None,
    find_project_result: Project | None,
) -> tuple[TaskRepository, RuntimeServiceImpl, SimpleNamespace]:
    repo = TaskRepository(tmp_path / "runtime.db")
    await repo.initialize()
    project_service_ns = SimpleNamespace(
        get_project=AsyncMock(return_value=get_project_result),
        find_project_by_repo_path=AsyncMock(return_value=find_project_result),
    )
    project_service = cast("ProjectService", project_service_ns)
    service = RuntimeServiceImpl(
        project_service=project_service,
        session_factory=repo.session_factory,
        execution_service=_mock_execution_service(),
    )
    return repo, service, project_service_ns


async def _make_runtime_view_service(tmp_path: Path):
    repo = TaskRepository(tmp_path / "runtime-view.db")
    await repo.initialize()
    project_service = cast("ProjectService", SimpleNamespace())
    execution_service = cast(
        "ExecutionRepository",
        SimpleNamespace(
            get_latest_execution_for_task=AsyncMock(return_value=None),
            get_latest_running_executions_for_tasks=AsyncMock(return_value={}),
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


async def test_prepare_auto_output_waiting_runtime_has_no_side_effects(tmp_path: Path) -> None:
    task = _Task(id="auto0001", task_type=TaskType.AUTO)
    runtime_view = RuntimeTaskView(task_id=task.id, phase=RuntimeTaskPhase.RUNNING)
    service, automation, executions, repo = await _make_service(
        tmp_path,
        runtime_view=runtime_view,
    )

    readiness = await service.prepare_auto_output(cast("TaskLike", task))

    assert readiness.can_open_output is True
    assert readiness.is_running is True
    assert readiness.recovered_stale_execution is False
    assert readiness.message == "Agent is starting. Opening output while live stream attaches."
    executions.update_execution.assert_not_awaited()
    automation.spawn_for_task.assert_not_awaited()
    await repo.close()


async def test_prepare_auto_output_prefers_runtime_view_when_view_is_running(
    tmp_path: Path,
) -> None:
    task = _Task(id="auto0001", task_type=TaskType.AUTO)
    runtime_agent = object()
    runtime_view = RuntimeTaskView(
        task_id=task.id,
        phase=RuntimeTaskPhase.RUNNING,
        execution_id="exec-view",
        run_count=2,
        running_agent=cast("Any", runtime_agent),
    )
    service, _automation, _executions, repo = await _make_service(
        tmp_path,
        runtime_view=runtime_view,
    )

    readiness = await service.prepare_auto_output(cast("TaskLike", task))

    assert readiness.is_running is True
    assert readiness.execution_id == "exec-view"
    assert readiness.running_agent is runtime_agent
    await repo.close()


async def test_prepare_auto_output_does_not_fallback_to_automation_when_view_is_missing(
    tmp_path: Path,
) -> None:
    task = _Task(id="auto0001", task_type=TaskType.AUTO)
    service, _automation, _executions, repo = await _make_service(
        tmp_path,
    )

    readiness = await service.prepare_auto_output(cast("TaskLike", task))

    assert readiness.is_running is False
    assert readiness.execution_id is None
    assert readiness.running_agent is None
    assert readiness.message == "No agent logs available for this task"
    await repo.close()


async def test_prepare_auto_output_clears_stale_runtime_execution_reference(tmp_path: Path) -> None:
    task = _Task(id="auto0001", task_type=TaskType.AUTO)
    runtime_view = RuntimeTaskView(
        task_id=task.id,
        phase=RuntimeTaskPhase.RUNNING,
        execution_id="exec-stale",
    )
    stale_execution = SimpleNamespace(id="exec-stale", status=ExecutionStatus.COMPLETED)
    service, _automation, _executions, repo = await _make_service(
        tmp_path,
        runtime_view=runtime_view,
        execution_by_id={"exec-stale": stale_execution},
    )

    readiness = await service.prepare_auto_output(cast("TaskLike", task))

    assert readiness.is_running is False
    assert readiness.output_mode is AutoOutputMode.UNAVAILABLE
    assert readiness.message == "No agent logs available for this task"
    assert service.get(task.id) is None
    await repo.close()


@pytest.mark.parametrize(
    ("status", "logs", "is_running", "should_recover"),
    [
        pytest.param(ExecutionStatus.RUNNING, [SimpleNamespace(logs="")], False, True, id="stale"),
        pytest.param(
            ExecutionStatus.RUNNING,
            [SimpleNamespace(logs='{"messages":[{"content":"ready"}]}')],
            False,
            False,
            id="running-with-logs",
        ),
        pytest.param(ExecutionStatus.RUNNING, [SimpleNamespace(logs="")], True, False, id="live"),
        pytest.param(
            ExecutionStatus.COMPLETED,
            [SimpleNamespace(logs="")],
            False,
            False,
            id="completed",
        ),
    ],
)
async def test_recover_stale_auto_output_only_recovers_for_true_stale_condition(
    tmp_path: Path,
    status: ExecutionStatus,
    logs: list[SimpleNamespace],
    is_running: bool,
    should_recover: bool,
) -> None:
    task = _Task(id="auto0001", task_type=TaskType.AUTO)
    latest = SimpleNamespace(id="exec-1", status=status)
    runtime_view = (
        RuntimeTaskView(task_id=task.id, phase=RuntimeTaskPhase.RUNNING) if is_running else None
    )
    service, automation, executions, repo = await _make_service(
        tmp_path,
        latest_execution=latest,
        log_entries=logs,
        runtime_view=runtime_view,
    )

    recovery = await service.recover_stale_auto_output(cast("TaskLike", task))

    if should_recover:
        executions.update_execution.assert_awaited_once()
        automation.spawn_for_task.assert_awaited_once_with(task)
        assert "Recovered stale execution" in recovery.message
    else:
        executions.update_execution.assert_not_awaited()
        automation.spawn_for_task.assert_not_awaited()
        assert recovery.message == "Stale AUTO output recovery is not required."
    await repo.close()


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
    assert service.state.project_id is None
    assert service.state.repo_id is None

    await service.set_last_active_context("proj-1", "repo-1")
    saved = await service.get_last_active_context()
    assert saved.project_id == "proj-1"
    assert saved.repo_id == "repo-1"
    assert service.state.project_id == "proj-1"
    assert service.state.repo_id == "repo-1"

    await service.set_last_active_context("proj-2", None)
    updated = await service.get_last_active_context()
    assert updated.project_id == "proj-2"
    assert updated.repo_id is None
    assert service.state.project_id == "proj-2"
    assert service.state.repo_id is None

    await repo.close()


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


async def test_reconcile_startup_state_sets_in_memory_state(tmp_path: Path) -> None:
    project = Project(id="proj-persisted", name="Persisted")
    repo, service, _ = await _make_runtime_service(
        tmp_path,
        get_project_result=project,
        find_project_result=None,
    )
    await service.set_last_active_context(project.id, "repo-persisted")

    try:
        reconciled = await service.reconcile_startup_state()
        assert reconciled == RuntimeContextState(project_id=project.id, repo_id="repo-persisted")
        assert service.state.project_id == project.id
        assert service.state.repo_id == "repo-persisted"
    finally:
        await repo.close()


async def test_reconcile_startup_state_clears_missing_project(tmp_path: Path) -> None:
    repo, service, _ = await _make_runtime_service(
        tmp_path,
        get_project_result=None,
        find_project_result=None,
    )
    await service.set_last_active_context("proj-missing", "repo-persisted")

    try:
        reconciled = await service.reconcile_startup_state()
        assert reconciled == RuntimeContextState(project_id=None, repo_id=None)
        assert service.state.project_id is None
        assert service.state.repo_id is None
        persisted = await service.get_last_active_context()
        assert persisted == RuntimeContextState(project_id=None, repo_id=None)
    finally:
        await repo.close()


@pytest.mark.parametrize(
    ("persisted_project", "cwd_project", "expect_project", "expect_repo", "expect_cleared"),
    [
        (
            Project(id="proj-persisted", name="Persisted"),
            None,
            "proj-persisted",
            "repo-persisted",
            False,
        ),
        (None, Project(id="proj-cwd", name="CWD"), "proj-cwd", None, True),
    ],
)
async def test_decide_startup_prefers_persisted_or_cwd_project(
    tmp_path: Path,
    persisted_project: Project | None,
    cwd_project: Project | None,
    expect_project: str,
    expect_repo: str | None,
    expect_cleared: bool,
) -> None:
    repo, service, project_service_ns = await _make_runtime_service(
        tmp_path,
        get_project_result=persisted_project,
        find_project_result=cwd_project,
    )
    await service.set_last_active_context("proj-persisted", "repo-persisted")

    try:
        decision = await service.decide_startup(tmp_path)

        assert decision.project_id == expect_project
        assert decision.preferred_repo_id == expect_repo
        if cwd_project is not None:
            assert decision.preferred_path == tmp_path
        if expect_cleared:
            persisted = await service.get_last_active_context()
            assert persisted == RuntimeContextState(project_id=None, repo_id=None)
        if persisted_project is not None:
            project_service_ns.find_project_by_repo_path.assert_not_awaited()
    finally:
        await repo.close()


async def test_decide_startup_falls_back_to_welcome(tmp_path: Path, monkeypatch) -> None:
    repo, service, _ = await _make_runtime_service(
        tmp_path,
        get_project_result=None,
        find_project_result=None,
    )
    monkeypatch.setattr("kagan.core.services.runtime.has_git_repo", AsyncMock(return_value=True))

    try:
        decision = await service.decide_startup(tmp_path)
        assert decision.project_id is None
        assert decision.suggest_cwd is True
        assert decision.cwd_path == str(tmp_path)
        assert decision.cwd_is_git_repo is True
    finally:
        await repo.close()


def test_empty_runtime_snapshot_defaults() -> None:
    snapshot = empty_runtime_snapshot()

    assert snapshot == {
        "is_running": False,
        "is_reviewing": False,
        "is_blocked": False,
        "blocked_reason": None,
        "blocked_by_task_ids": [],
        "overlap_hints": [],
        "blocked_at": None,
        "is_pending": False,
        "pending_reason": None,
        "pending_at": None,
    }


def test_serialize_runtime_view_populates_all_known_fields() -> None:
    blocked_at = datetime(2026, 2, 10, 12, 1, 2, tzinfo=UTC)
    pending_at = datetime(2026, 2, 10, 12, 3, 4, tzinfo=UTC)
    view = SimpleNamespace(
        is_running=True,
        is_reviewing=False,
        is_blocked=True,
        blocked_reason="conflict with T2",
        blocked_by_task_ids=("T2",),
        overlap_hints=("src/app.py", "README.md"),
        blocked_at=blocked_at,
        is_pending=True,
        pending_reason="awaiting slot",
        pending_at=pending_at,
    )

    snapshot = serialize_runtime_view(view)

    assert snapshot["is_running"] is True
    assert snapshot["is_blocked"] is True
    assert snapshot["blocked_reason"] == "conflict with T2"
    assert snapshot["blocked_by_task_ids"] == ["T2"]
    assert snapshot["overlap_hints"] == ["src/app.py", "README.md"]
    assert snapshot["blocked_at"] == blocked_at.isoformat()
    assert snapshot["is_pending"] is True
    assert snapshot["pending_reason"] == "awaiting slot"
    assert snapshot["pending_at"] == pending_at.isoformat()


def test_runtime_snapshot_for_task_handles_missing_runtime_service() -> None:
    snapshot = runtime_snapshot_for_task(task_id="T1", runtime_service=None)
    assert snapshot == empty_runtime_snapshot()


def test_runtime_snapshot_for_task_uses_runtime_service_view() -> None:
    runtime_service = SimpleNamespace(
        get=lambda _task_id: SimpleNamespace(
            is_running=True,
            blocked_by_task_ids=["T2"],
            overlap_hints=[],
        )
    )

    snapshot = runtime_snapshot_for_task(
        task_id="T1",
        runtime_service=cast("RuntimeSnapshotSource", runtime_service),
    )
    assert snapshot["is_running"] is True
    assert snapshot["blocked_by_task_ids"] == ["T2"]


async def test_runtime_task_view_tracks_running_and_reviewing_agents(tmp_path: Path) -> None:
    service, repo = await _make_runtime_view_service(tmp_path)
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
    service, repo = await _make_runtime_view_service(tmp_path)
    service.mark_started("task-1")
    assert service.running_tasks() == {"task-1"}

    service.mark_ended("task-1")
    assert service.get("task-1") is None
    assert service.running_tasks() == set()
    await repo.close()


async def test_reconcile_running_tasks_tracks_persisted_running_state(tmp_path: Path) -> None:
    service, repo = await _make_runtime_view_service(tmp_path)
    execution_repo = cast("SimpleNamespace", service._executions)
    execution_repo.get_latest_running_executions_for_tasks.return_value = {"task-1": "exec-1"}

    await service.reconcile_running_tasks(["task-1", "task-2"])

    view = service.get("task-1")
    assert view is not None
    assert view.phase == RuntimeTaskPhase.RUNNING
    assert view.execution_id == "exec-1"
    assert service.get("task-2") is None

    execution_repo.get_latest_running_executions_for_tasks.return_value = {}
    await service.reconcile_running_tasks(["task-1"])
    assert service.get("task-1") is None
    await repo.close()


async def test_runtime_task_view_tracks_blocked_state(tmp_path: Path) -> None:
    service, repo = await _make_runtime_view_service(tmp_path)

    service.mark_blocked(
        "task-1",
        reason="Waiting on #task-0",
        blocked_by_task_ids=("task-0",),
        overlap_hints=("src/calculator.py",),
    )
    view = service.get("task-1")
    assert view is not None
    assert view.is_blocked is True
    assert view.blocked_by_task_ids == ("task-0",)
    assert service.running_tasks() == set()

    service.clear_blocked("task-1")
    assert service.get("task-1") is None
    await repo.close()
