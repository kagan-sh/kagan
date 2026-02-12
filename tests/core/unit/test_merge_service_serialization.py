from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, cast
from unittest.mock import AsyncMock

from kagan.core.config import KaganConfig
from kagan.core.models.enums import TaskStatus
from kagan.core.services.merges import MergeResult, MergeServiceImpl, MergeStrategy

if TYPE_CHECKING:
    from kagan.core.services.automation import AutomationService
    from kagan.core.services.sessions import SessionService
    from kagan.core.services.tasks import TaskService
    from kagan.core.services.workspaces import WorkspaceService


class _RecordingLock:
    def __init__(self, events: list[str]) -> None:
        self._events = events

    async def __aenter__(self) -> _RecordingLock:
        self._events.append("lock_enter")
        return self

    async def __aexit__(self, *args: object) -> None:
        del args
        self._events.append("lock_exit")


def _build_service(
    *,
    serialize_merges: bool,
    events: list[str],
    merge_batches: list[list[MergeResult]] | None = None,
    rebase_result: tuple[bool, str, list[str]] = (True, "ok", []),
    repos: list[dict[str, object]] | None = None,
    commits: list[str] | None = None,
    changed_files: list[str] | None = None,
    base_changed_files: list[str] | None = None,
    running: bool = False,
    reviewing: bool = False,
    stop_clears_runtime: bool = True,
) -> tuple[MergeServiceImpl, Any, Any]:
    workspace_repos = repos or [
        {
            "repo_id": "repo-1",
            "repo_name": "repo-1",
            "has_changes": True,
            "target_branch": "main",
        }
    ]
    task_service = SimpleNamespace(update_fields=AsyncMock())
    worktrees = SimpleNamespace(
        release=AsyncMock(),
        rebase_onto_base=AsyncMock(return_value=rebase_result),
        get_workspace_repos=AsyncMock(return_value=workspace_repos),
        get_commit_log=AsyncMock(return_value=commits or []),
        get_files_changed=AsyncMock(return_value=changed_files or ["repo-1:file.py"]),
        get_files_changed_on_base=AsyncMock(return_value=base_changed_files or []),
    )
    sessions = SimpleNamespace(kill_session=AsyncMock())
    runtime_state = {"running": running, "reviewing": reviewing}

    async def _stop_task(_task_id: str) -> bool:
        events.append("stop_task")
        if stop_clears_runtime:
            runtime_state["running"] = False
            runtime_state["reviewing"] = False
        return True

    automation = SimpleNamespace(
        merge_lock=_RecordingLock(events),
        stop_task=AsyncMock(side_effect=_stop_task),
        is_running=lambda _task_id: runtime_state["running"],
        is_reviewing=lambda _task_id: runtime_state["reviewing"],
    )
    config = KaganConfig()
    config.general.serialize_merges = serialize_merges

    service = MergeServiceImpl(
        task_service=cast("TaskService", task_service),
        worktrees=cast("WorkspaceService", worktrees),
        sessions=cast("SessionService", sessions),
        automation=cast("AutomationService", automation),
        config=config,
    )

    service._get_latest_workspace_id = AsyncMock(return_value="ws-1")
    service.has_no_changes = AsyncMock(return_value=False)
    batches = list(merge_batches or [])

    async def _merge_all(*args: object, **kwargs: object) -> list[MergeResult]:
        del args, kwargs
        events.append("merge_all")
        if batches:
            return batches.pop(0)
        return [_success_result()]

    service.merge_all = AsyncMock(side_effect=_merge_all)
    task = SimpleNamespace(id="task-1", title="Test task", description="desc", base_branch="main")
    return service, task, worktrees


def _success_result() -> MergeResult:
    return MergeResult(
        repo_id="repo-1",
        repo_name="repo-1",
        strategy=MergeStrategy.DIRECT,
        success=True,
        message="ok",
    )


def _base_ahead_failure() -> MergeResult:
    return MergeResult(
        repo_id="repo-1",
        repo_name="repo-1",
        strategy=MergeStrategy.DIRECT,
        success=False,
        message="Base branch main is ahead of task-1; rebase required",
    )


def _conflict_failure() -> MergeResult:
    return MergeResult(
        repo_id="repo-1",
        repo_name="repo-1",
        strategy=MergeStrategy.DIRECT,
        success=False,
        message="Merge conflict detected",
    )


async def test_merge_task_serializes_when_enabled() -> None:
    events: list[str] = []
    service, task, _worktrees = _build_service(serialize_merges=True, events=events)

    success, message = await service.merge_task(task)

    assert success is True
    assert message == "Merged all repos"
    assert events == ["lock_enter", "merge_all", "lock_exit"]
    service.tasks.update_fields.assert_awaited_once_with(task.id, status=TaskStatus.DONE)


async def test_merge_task_does_not_take_lock_when_disabled() -> None:
    events: list[str] = []
    service, task, _worktrees = _build_service(serialize_merges=False, events=events)

    success, message = await service.merge_task(task)

    assert success is True
    assert message == "Merged all repos"
    assert events == ["merge_all"]
    service.tasks.update_fields.assert_awaited_once_with(task.id, status=TaskStatus.DONE)


async def test_merge_task_preemptively_rebases_for_high_overlap_risk() -> None:
    events: list[str] = []
    service, task, worktrees = _build_service(
        serialize_merges=True,
        events=events,
        merge_batches=[[_success_result()]],
        changed_files=["repo-1:src/calc.py"],
        base_changed_files=["repo-1:src/calc.py"],
    )

    success, message = await service.merge_task(task)

    assert success is True
    assert message == "Merged all repos (after pre-merge rebase)"
    worktrees.rebase_onto_base.assert_awaited_once_with(task.id, "main")
    assert events == ["lock_enter", "merge_all", "lock_exit"]


async def test_merge_task_auto_rebases_and_retries_when_base_is_ahead() -> None:
    events: list[str] = []
    service, task, worktrees = _build_service(
        serialize_merges=True,
        events=events,
        merge_batches=[[_base_ahead_failure()], [_success_result()]],
    )

    success, message = await service.merge_task(task)

    assert success is True
    assert message == "Merged all repos (after auto-rebase)"
    assert events == ["lock_enter", "merge_all", "merge_all", "lock_exit"]
    assert service.merge_all.await_count == 2
    worktrees.rebase_onto_base.assert_awaited_once_with(task.id, "main")


async def test_merge_task_fails_when_auto_rebase_fails() -> None:
    events: list[str] = []
    service, task, worktrees = _build_service(
        serialize_merges=True,
        events=events,
        merge_batches=[[_base_ahead_failure()]],
        rebase_result=(False, "Rebase conflict in repo-1", ["repo-1:file.py"]),
    )

    success, message = await service.merge_task(task)

    assert success is False
    assert message == "Merge blocked: Rebase conflict in repo-1"
    assert service.merge_all.await_count == 1
    worktrees.rebase_onto_base.assert_awaited_once_with(task.id, "main")


async def test_merge_task_skips_auto_rebase_for_non_rebase_failures() -> None:
    events: list[str] = []
    service, task, worktrees = _build_service(
        serialize_merges=True,
        events=events,
        merge_batches=[[_conflict_failure()]],
    )

    success, message = await service.merge_task(task)

    assert success is False
    assert message.startswith("repo-1: Merge conflict detected")
    assert "Tip: run review rebase, resolve conflicts, then merge again" in message
    assert service.merge_all.await_count == 1
    worktrees.rebase_onto_base.assert_not_awaited()


async def test_merge_task_reuses_rebase_hint_for_next_merge_attempt() -> None:
    events: list[str] = []
    service, task, worktrees = _build_service(
        serialize_merges=True,
        events=events,
        merge_batches=[
            [_base_ahead_failure()],
            [_success_result()],
            [_success_result()],
        ],
        changed_files=["repo-1:file.py"],
        base_changed_files=[],
    )

    first_success, first_message = await service.merge_task(task)
    second_success, second_message = await service.merge_task(task)

    assert first_success is True
    assert first_message == "Merged all repos (after auto-rebase)"
    assert second_success is True
    assert second_message == "Merged all repos (after pre-merge rebase)"
    assert worktrees.rebase_onto_base.await_count == 2


async def test_merge_task_stops_active_runtime_before_merging() -> None:
    events: list[str] = []
    service, task, _worktrees = _build_service(
        serialize_merges=True,
        events=events,
        running=True,
    )

    success, message = await service.merge_task(task)

    assert success is True
    assert message == "Merged all repos"
    assert events == ["lock_enter", "stop_task", "merge_all", "lock_exit"]


async def test_merge_task_fails_when_runtime_does_not_quiesce(monkeypatch) -> None:
    events: list[str] = []
    service, task, _worktrees = _build_service(
        serialize_merges=True,
        events=events,
        reviewing=True,
        stop_clears_runtime=False,
    )
    monkeypatch.setattr(service, "_MERGE_QUIESCE_TIMEOUT_SECONDS", 0.01)
    monkeypatch.setattr(service, "_MERGE_QUIESCE_POLL_SECONDS", 0.005)

    success, message = await service.merge_task(task)

    assert success is False
    assert message.startswith("Merge blocked: Task runtime is still active")
    assert events == ["lock_enter", "stop_task", "lock_exit"]
