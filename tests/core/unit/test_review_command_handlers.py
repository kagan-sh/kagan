"""Tests for review api adapter functions and api review methods."""

from __future__ import annotations

from types import SimpleNamespace
from typing import Any
from unittest.mock import AsyncMock

from kagan.core.api import KaganAPI
from kagan.core.models.enums import TaskStatus, TaskType
from kagan.core.request_handlers import (
    handle_review_approve,
    handle_review_merge,
    handle_review_rebase,
)


def _api(**services: object) -> KaganAPI:
    from typing import cast

    ctx = SimpleNamespace(**services)
    return KaganAPI(cast("Any", ctx))


async def test_review_merge_returns_not_found_when_task_missing() -> None:
    f = _api(task_service=SimpleNamespace(get_task=AsyncMock(return_value=None)))
    result = await handle_review_merge(f, {"task_id": "task-1"})
    assert result["success"] is False
    assert result["task_id"] == "task-1"
    assert "not found" in result["message"]
    assert result["code"] == "MERGE_FAILED"


async def test_review_merge_requires_approval_when_enabled() -> None:
    task = SimpleNamespace(id="task-1", status=TaskStatus.REVIEW)
    merge_service = SimpleNamespace(merge_task=AsyncMock(return_value=(True, "Merged all repos")))
    f = _api(
        task_service=SimpleNamespace(get_task=AsyncMock(return_value=task)),
        merge_service=merge_service,
        config=SimpleNamespace(general=SimpleNamespace(require_review_approval=True)),
    )
    result = await handle_review_merge(f, {"task_id": "task-1"})
    assert result["success"] is False
    assert result["task_id"] == "task-1"
    assert "approved before merge" in result["message"]
    assert result["code"] == "MERGE_FAILED"
    merge_service.merge_task.assert_not_awaited()


async def test_review_merge_allows_approved_task_when_gate_enabled() -> None:
    task = SimpleNamespace(id="task-1", status=TaskStatus.REVIEW)
    execution = SimpleNamespace(id="exec-1", metadata_={"review_result": {"status": "approved"}})
    merge_service = SimpleNamespace(merge_task=AsyncMock(return_value=(True, "Merged all repos")))
    f = _api(
        task_service=SimpleNamespace(get_task=AsyncMock(return_value=task)),
        execution_service=SimpleNamespace(get_latest_execution_for_task=AsyncMock(return_value=execution)),
        merge_service=merge_service,
        config=SimpleNamespace(general=SimpleNamespace(require_review_approval=True)),
    )
    result = await handle_review_merge(f, {"task_id": "task-1"})
    assert result["success"] is True
    assert result["task_id"] == "task-1"
    assert result["message"] == "Merged all repos"
    assert result["code"] == "MERGED"
    merge_service.merge_task.assert_awaited_once_with(task)


async def test_review_approve_keeps_task_in_review() -> None:
    task = SimpleNamespace(id="task-1", status=TaskStatus.REVIEW)
    task_service = SimpleNamespace(
        get_task=AsyncMock(return_value=task),
    )
    execution = SimpleNamespace(id="exec-1", metadata_={})
    execution_service = SimpleNamespace(
        get_latest_execution_for_task=AsyncMock(return_value=execution),
        update_execution=AsyncMock(return_value=execution),
    )
    f = _api(
        task_service=task_service,
        execution_service=execution_service,
    )

    result = await handle_review_approve(f, {"task_id": "task-1"})

    assert result["success"] is True
    assert result["code"] == "APPROVED"
    assert result["status"] == "approved"
    assert result["task_status"] == TaskStatus.REVIEW.value
    execution_service.update_execution.assert_awaited_once()


async def test_review_approve_rejects_non_review_state() -> None:
    task = SimpleNamespace(id="task-1", status=TaskStatus.IN_PROGRESS)
    f = _api(task_service=SimpleNamespace(get_task=AsyncMock(return_value=task)))

    result = await handle_review_approve(f, {"task_id": "task-1"})

    assert result["success"] is False
    assert result["task_id"] == "task-1"
    assert result["code"] == "REVIEW_NOT_READY"


async def test_review_merge_restores_review_status_on_failed_merge_transition() -> None:
    task = SimpleNamespace(id="task-1", status=TaskStatus.REVIEW)
    task_after_failure = SimpleNamespace(id="task-1", status=TaskStatus.DONE)
    merge_service = SimpleNamespace(
        merge_task=AsyncMock(return_value=(False, "Merge blocked: conflict"))
    )
    task_service = SimpleNamespace(
        get_task=AsyncMock(side_effect=[task, task_after_failure]),
        move=AsyncMock(return_value=task),
    )
    f = _api(
        task_service=task_service,
        merge_service=merge_service,
        config=SimpleNamespace(general=SimpleNamespace(require_review_approval=False)),
    )
    result = await handle_review_merge(f, {"task_id": "task-1"})
    assert result["success"] is False
    assert result["code"] == "MERGE_FAILED"
    assert "Task returned to REVIEW" in result["message"]
    task_service.move.assert_awaited_once_with("task-1", TaskStatus.REVIEW)


async def test_review_rebase_returns_not_found_when_task_missing() -> None:
    f = _api(task_service=SimpleNamespace(get_task=AsyncMock(return_value=None)))
    result = await handle_review_rebase(f, {"task_id": "task-1"})
    assert result["success"] is False
    assert result["task_id"] == "task-1"
    assert "not found" in result["message"]


async def test_review_rebase_requires_review_status() -> None:
    task = SimpleNamespace(
        id="task-1",
        title="Task 1",
        status=TaskStatus.IN_PROGRESS,
        base_branch="main",
        description="",
        task_type=TaskType.AUTO,
    )
    f = _api(task_service=SimpleNamespace(get_task=AsyncMock(return_value=task)))
    result = await handle_review_rebase(f, {"task_id": "task-1"})
    assert result["success"] is False
    assert result["task_id"] == "task-1"
    assert "not in REVIEW" in result["message"]


async def test_review_rebase_success_matches_tui_flow() -> None:
    task = SimpleNamespace(
        id="task-1",
        title="Task 1",
        status=TaskStatus.REVIEW,
        base_branch="main",
        description="",
        task_type=TaskType.AUTO,
    )
    workspace_service = SimpleNamespace(
        rebase_onto_base=AsyncMock(return_value=(True, "ok", [])),
    )
    f = _api(
        task_service=SimpleNamespace(get_task=AsyncMock(return_value=task)),
        workspace_service=workspace_service,
        config=SimpleNamespace(general=SimpleNamespace(default_base_branch="main")),
    )
    result = await handle_review_rebase(f, {"task_id": "task-1"})
    assert result["success"] is True
    assert result["task_id"] == "task-1"
    assert "Rebased" in result["message"]
    assert result["conflict_files"] == []
    assert result["code"] == "REBASED"
    workspace_service.rebase_onto_base.assert_awaited_once_with("task-1", "main")


async def test_review_rebase_conflict_moves_task_to_in_progress_and_respawns_auto() -> None:
    task = SimpleNamespace(
        id="task-1",
        title="Task 1",
        status=TaskStatus.REVIEW,
        base_branch="main",
        description="Existing notes",
        task_type=TaskType.AUTO,
    )
    refreshed = SimpleNamespace(id="task-1", task_type=TaskType.AUTO)
    task_service = SimpleNamespace(
        get_task=AsyncMock(side_effect=[task, refreshed]),
        update_fields=AsyncMock(return_value=task),
        move=AsyncMock(return_value=task),
    )
    workspace_service = SimpleNamespace(
        rebase_onto_base=AsyncMock(
            return_value=(False, "conflict", ["repo-a:src/conflict.py", "repo-b:README.md"])
        ),
        abort_rebase=AsyncMock(return_value=(True, "aborted")),
    )
    automation_service = SimpleNamespace(spawn_for_task=AsyncMock(return_value=True))
    f = _api(
        task_service=task_service,
        workspace_service=workspace_service,
        automation_service=automation_service,
        config=SimpleNamespace(general=SimpleNamespace(default_base_branch="main")),
    )
    result = await handle_review_rebase(f, {"task_id": "task-1"})
    assert result["success"] is False
    assert result["task_id"] == "task-1"
    assert result["conflict_files"] == ["repo-a:src/conflict.py", "repo-b:README.md"]
    assert result["code"] == "REBASE_CONFLICT"

    workspace_service.abort_rebase.assert_awaited_once_with("task-1")
    task_service.move.assert_awaited_once_with("task-1", TaskStatus.IN_PROGRESS)
    assert task_service.update_fields.await_count == 1
    description_value = task_service.update_fields.await_args.kwargs["description"]
    assert "Rebase conflict detected" in description_value
    automation_service.spawn_for_task.assert_awaited_once_with(refreshed)
