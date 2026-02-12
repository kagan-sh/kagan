"""Tests for task delete api adapter function (formerly CQRS handler)."""

from __future__ import annotations

from datetime import UTC, datetime
from types import SimpleNamespace
from typing import Any
from unittest.mock import AsyncMock

import pytest

from kagan.core.api import KaganAPI
from kagan.core.events import TaskDeleted
from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
from kagan.core.request_handlers import handle_task_context, handle_task_delete, handle_task_logs


def _api(**services: object) -> KaganAPI:
    from typing import cast

    ctx = SimpleNamespace(**services)
    return KaganAPI(cast("Any", ctx))


def _task(task_id: str, *, status: TaskStatus) -> SimpleNamespace:
    now = datetime.now(tz=UTC)
    return SimpleNamespace(
        id=task_id,
        project_id="proj-1",
        parent_id=None,
        title=f"Task {task_id}",
        description=f"Description {task_id}",
        status=status,
        priority=TaskPriority.MEDIUM,
        task_type=TaskType.PAIR,
        terminal_backend=None,
        agent_backend=None,
        acceptance_criteria=["criterion"],
        base_branch="main",
        created_at=now,
        updated_at=now,
    )


async def test_delete_task_uses_merge_service_cleanup() -> None:
    task = SimpleNamespace(id="task-1")
    task_service = SimpleNamespace(get_task=AsyncMock(return_value=task))
    merge_service = SimpleNamespace(
        delete_task=AsyncMock(return_value=(True, "Deleted successfully"))
    )
    f = _api(task_service=task_service, merge_service=merge_service)

    result = await handle_task_delete(f, {"task_id": "task-1"})

    assert result == {"success": True, "task_id": "task-1", "message": "Deleted successfully"}
    merge_service.delete_task.assert_awaited_once_with(task)


async def test_delete_task_fallback_cleans_runtime_resources() -> None:
    task = SimpleNamespace(id="task-1")
    task_service = SimpleNamespace(
        get_task=AsyncMock(return_value=task),
        delete_task=AsyncMock(return_value=True),
    )
    automation_service = SimpleNamespace(
        is_running=lambda _task_id: True,
        stop_task=AsyncMock(return_value=True),
    )
    session_service = SimpleNamespace(
        session_exists=AsyncMock(return_value=True),
        kill_session=AsyncMock(return_value=None),
    )
    workspace_service = SimpleNamespace(
        get_path=AsyncMock(return_value="/tmp/worktree"),
        delete=AsyncMock(return_value=None),
    )
    f = _api(
        task_service=task_service,
        automation_service=automation_service,
        session_service=session_service,
        workspace_service=workspace_service,
        merge_service=None,
    )

    result = await handle_task_delete(f, {"task_id": "task-1"})

    assert result == {"success": True, "task_id": "task-1", "message": "Deleted successfully"}
    automation_service.stop_task.assert_awaited_once_with("task-1")
    session_service.session_exists.assert_awaited_once_with("task-1")
    session_service.kill_session.assert_awaited_once_with("task-1")
    workspace_service.get_path.assert_awaited_once_with("task-1")
    workspace_service.delete.assert_awaited_once_with("task-1", delete_branch=True)
    task_service.delete_task.assert_awaited_once_with("task-1")


async def test_task_context_falls_back_to_project_repos_on_workspace_failure() -> None:
    """When workspace repos fail, api falls back to project repos."""
    task = _task("task-1", status=TaskStatus.IN_PROGRESS)

    task_service = SimpleNamespace(
        get_task=AsyncMock(return_value=task),
        get_scratchpad=AsyncMock(return_value="scratch"),
        get_task_links=AsyncMock(return_value=[]),
    )
    workspace_service = SimpleNamespace(
        list_workspaces=AsyncMock(
            return_value=[
                SimpleNamespace(id="ws-1", branch_name="feature/task-1", path="/tmp/ws-1"),
            ]
        ),
        get_workspace_repos=AsyncMock(side_effect=OSError("workspace repos unavailable")),
    )
    project_repo = SimpleNamespace(
        id="repo-1", name="repo", path="/tmp/repo", default_branch="main"
    )
    project_service = SimpleNamespace(get_project_repos=AsyncMock(return_value=[project_repo]))
    f = _api(
        task_service=task_service,
        workspace_service=workspace_service,
        project_service=project_service,
    )

    result = await handle_task_context(f, {"task_id": "task-1"})

    assert result["repo_count"] == 1
    assert result["repos"][0]["repo_id"] == "repo-1"
    assert result["workspace_id"] == "ws-1"


async def test_task_context_returns_empty_repos_when_both_sources_fail() -> None:
    """When both workspace and project repos fail, repos list is empty."""
    task = _task("task-1", status=TaskStatus.IN_PROGRESS)

    task_service = SimpleNamespace(
        get_task=AsyncMock(return_value=task),
        get_scratchpad=AsyncMock(return_value="scratch"),
        get_task_links=AsyncMock(return_value=[]),
    )
    workspace_service = SimpleNamespace(
        list_workspaces=AsyncMock(
            return_value=[
                SimpleNamespace(id="ws-1", branch_name="feature/task-1", path="/tmp/ws-1"),
            ]
        ),
        get_workspace_repos=AsyncMock(side_effect=OSError("workspace repos unavailable")),
    )
    project_service = SimpleNamespace(
        get_project_repos=AsyncMock(side_effect=RuntimeError("project repos unavailable"))
    )
    f = _api(
        task_service=task_service,
        workspace_service=workspace_service,
        project_service=project_service,
    )

    result = await handle_task_context(f, {"task_id": "task-1"})

    assert result["repo_count"] == 0
    assert result["repos"] == []


async def test_task_logs_returns_empty_on_fetch_failures() -> None:
    """When execution log entries fail, logs list is empty."""
    execution = SimpleNamespace(id="exec-1", created_at=datetime.now(tz=UTC))
    execution_service = SimpleNamespace(
        list_executions_for_task=AsyncMock(return_value=[execution]),
        count_executions_for_task=AsyncMock(side_effect=RuntimeError("count unavailable")),
        get_execution_log_entries=AsyncMock(side_effect=RuntimeError("log storage unavailable")),
    )
    f = _api(execution_service=execution_service)

    result = await handle_task_logs(f, {"task_id": "task-1", "limit": 5})

    assert result["task_id"] == "task-1"
    assert result["count"] == 0
    assert result["logs"] == []


async def test_delete_task_publishes_task_deleted_event(
    event_bus,
    state_manager,
    task_factory,
    task_service,
) -> None:
    task = await state_manager.create(task_factory(title="Delete me"))
    published: list[object] = []
    event_bus.add_handler(lambda event: published.append(event))

    deleted = await task_service.delete_task(task.id)

    assert deleted is True
    deleted_events = [event for event in published if isinstance(event, TaskDeleted)]
    assert len(deleted_events) == 1
    assert deleted_events[0].task_id == task.id


async def test_delete_missing_task_does_not_publish_task_deleted_event(
    event_bus,
    task_service,
) -> None:
    published: list[object] = []
    event_bus.add_handler(lambda event: published.append(event))

    deleted = await task_service.delete_task("missing-task-id")

    assert deleted is False
    assert not any(isinstance(event, TaskDeleted) for event in published)


@pytest.mark.parametrize(
    ("initial_status", "success", "expected"),
    [
        (TaskStatus.IN_PROGRESS, True, TaskStatus.REVIEW),
        (TaskStatus.IN_PROGRESS, False, TaskStatus.IN_PROGRESS),
        (TaskStatus.REVIEW, False, TaskStatus.REVIEW),
        (TaskStatus.BACKLOG, True, TaskStatus.BACKLOG),
        (TaskStatus.DONE, True, TaskStatus.DONE),
    ],
)
async def test_sync_status_from_agent_complete(
    state_manager,
    task_factory,
    task_service,
    initial_status: TaskStatus,
    success: bool,
    expected: TaskStatus,
) -> None:
    task = task_factory(title="Workflow test", status=initial_status)
    created = await state_manager.create(task)

    updated = await task_service.sync_status_from_agent_complete(created.id, success=success)
    assert updated is not None
    assert updated.status is expected


@pytest.mark.parametrize(
    ("initial_status", "expected"),
    [
        (TaskStatus.REVIEW, TaskStatus.DONE),
        (TaskStatus.BACKLOG, TaskStatus.BACKLOG),
        (TaskStatus.DONE, TaskStatus.DONE),
    ],
)
async def test_sync_status_from_review_pass(
    state_manager,
    task_factory,
    task_service,
    initial_status: TaskStatus,
    expected: TaskStatus,
) -> None:
    task = task_factory(title="Review pass transition", status=initial_status)
    created = await state_manager.create(task)

    updated = await task_service.sync_status_from_review_pass(created.id)
    assert updated is not None
    assert updated.status is expected


@pytest.mark.parametrize(
    ("initial_status", "expected"),
    [
        (TaskStatus.REVIEW, TaskStatus.IN_PROGRESS),
        (TaskStatus.BACKLOG, TaskStatus.BACKLOG),
        (TaskStatus.DONE, TaskStatus.DONE),
    ],
)
async def test_sync_status_from_review_reject(
    state_manager,
    task_factory,
    task_service,
    initial_status: TaskStatus,
    expected: TaskStatus,
) -> None:
    task = task_factory(title="Review reject transition", status=initial_status)
    created = await state_manager.create(task)

    updated = await task_service.sync_status_from_review_reject(created.id, reason="needs work")
    assert updated is not None
    assert updated.status is expected
