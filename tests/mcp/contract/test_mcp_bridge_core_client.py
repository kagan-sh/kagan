"""Tests for CoreClientBridge -- verifies IPC request translation."""

from __future__ import annotations

from unittest.mock import AsyncMock

import pytest
from tests.mcp.contract._bridge_test_support import SESSION, make_client

from kagan.core.ipc.contracts import CoreErrorDetail, CoreResponse
from kagan.mcp.tools import CoreClientBridge, MCPBridgeError


@pytest.mark.asyncio
async def test_get_context_uses_tasks_context_when_available() -> None:
    """get_context should prefer the richer tasks.context query when available."""
    client = make_client(
        {
            "task_id": "TASK-001",
            "title": "Test task",
            "description": "A test",
            "acceptance_criteria": ["criterion 1"],
            "scratchpad": "some notes",
            "workspace_id": "ws-1",
            "workspace_branch": "kagan/ws-1",
            "workspace_path": "/tmp/ws-1",
            "working_dir": "/tmp/ws-1/repo",
            "repos": [
                {
                    "repo_id": "repo-1",
                    "name": "Repo 1",
                    "path": "/tmp/repo-1",
                    "worktree_path": "/tmp/ws-1/repo",
                    "target_branch": "main",
                    "has_changes": True,
                    "diff_stats": "files=1, +2/-1",
                }
            ],
            "repo_count": 1,
            "linked_tasks": [
                {
                    "task_id": "TASK-XYZ1",
                    "title": "Linked",
                    "status": "in_progress",
                    "description": "linked task",
                }
            ],
        }
    )
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.get_context("TASK-001")

    assert result["task_id"] == "TASK-001"
    assert result["title"] == "Test task"
    assert result["scratchpad"] == "some notes"
    assert result["repo_count"] == 1
    assert len(result["linked_tasks"]) == 1
    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="tasks",
        method="context",
        params={"task_id": "TASK-001"},
    )


@pytest.mark.asyncio
async def test_get_context_raises_when_tasks_context_unavailable() -> None:
    """get_context should surface core errors when tasks.context is unavailable."""
    client = AsyncMock()
    calls: list[tuple[str, str]] = []

    async def mock_request(
        *,
        session_id,
        session_profile,
        session_origin,
        capability,
        method,
        params,
    ):
        calls.append((capability, method))
        assert session_id == SESSION
        assert session_profile is None
        assert session_origin is None
        if capability == "tasks" and method == "context":
            return CoreResponse(
                request_id="r0",
                ok=False,
                error=CoreErrorDetail(code="UNKNOWN_METHOD", message="No handler"),
            )
        return CoreResponse(request_id="rx", ok=True, result={})

    client.request = mock_request

    bridge = CoreClientBridge(client, SESSION)
    with pytest.raises(MCPBridgeError) as exc_info:
        await bridge.get_context("TASK-001")

    assert exc_info.value.code == "UNKNOWN_METHOD"
    assert calls == [("tasks", "context")]


@pytest.mark.asyncio
async def test_get_task_basic() -> None:
    """get_task should translate to tasks.get query."""
    client = make_client(
        {
            "found": True,
            "task": {
                "id": "TASK-002",
                "title": "Another task",
                "status": "in_progress",
                "description": "desc",
                "acceptance_criteria": [],
            },
        }
    )
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.get_task("TASK-002")

    assert result["task_id"] == "TASK-002"
    assert result["status"] == "in_progress"
    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="tasks",
        method="get",
        params={"task_id": "TASK-002"},
    )


@pytest.mark.asyncio
async def test_get_task_includes_runtime_metadata() -> None:
    client = make_client(
        {
            "found": True,
            "task": {
                "id": "TASK-RT1",
                "title": "Runtime task",
                "status": "backlog",
                "description": "desc",
                "acceptance_criteria": [],
                "runtime": {
                    "is_running": False,
                    "is_reviewing": False,
                    "is_blocked": True,
                    "blocked_reason": "Waiting on #abcd1234",
                    "blocked_by_task_ids": ["abcd1234"],
                    "overlap_hints": ["src/calculator.py"],
                    "blocked_at": "2026-02-10T09:00:00Z",
                },
            },
        }
    )
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.get_task("TASK-RT1")

    assert result["runtime"]["is_blocked"] is True
    assert result["runtime"]["blocked_by_task_ids"] == ["abcd1234"]
    assert result["runtime"]["blocked_at"] == "2026-02-10T09:00:00Z"


@pytest.mark.asyncio
async def test_get_task_with_scratchpad() -> None:
    """get_task with include_scratchpad should make two requests."""
    client = AsyncMock()
    calls: list[tuple[str, str, dict[str, object] | None]] = []

    async def mock_request(
        *,
        session_id,
        session_profile,
        session_origin,
        capability,
        method,
        params,
    ):
        calls.append((capability, method, params))
        assert session_profile is None
        assert session_origin is None
        if method == "get":
            return CoreResponse(
                request_id="r1",
                ok=True,
                result={
                    "found": True,
                    "task": {
                        "id": "T1",
                        "title": "T",
                        "status": "backlog",
                        "description": None,
                        "acceptance_criteria": None,
                    },
                },
            )
        if method == "scratchpad":
            return CoreResponse(
                request_id="r2",
                ok=True,
                result={"content": "notes"},
            )
        return CoreResponse(request_id="r0", ok=True, result={})

    client.request = mock_request
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.get_task("T1", include_scratchpad=True)

    assert result["task_id"] == "T1"
    assert result["scratchpad"] == "notes"
    assert calls == [
        ("tasks", "get", {"task_id": "T1"}),
        ("tasks", "scratchpad", {"task_id": "T1"}),
    ]


@pytest.mark.asyncio
async def test_get_task_with_logs_uses_tasks_logs() -> None:
    """get_task(include_logs=True) should query tasks.logs and return entries."""
    client = AsyncMock()
    calls: list[tuple[str, str]] = []

    async def mock_request(
        *,
        session_id,
        session_profile,
        session_origin,
        capability,
        method,
        params,
    ):
        calls.append((capability, method))
        assert session_id == SESSION
        assert session_profile is None
        assert session_origin is None
        if capability == "tasks" and method == "get":
            return CoreResponse(
                request_id="r1",
                ok=True,
                result={
                    "found": True,
                    "task": {
                        "id": "T1",
                        "title": "T",
                        "status": "backlog",
                        "description": None,
                        "acceptance_criteria": [],
                    },
                },
            )
        if capability == "tasks" and method == "logs":
            return CoreResponse(
                request_id="r2",
                ok=True,
                result={
                    "task_id": "T1",
                    "logs": [
                        {
                            "run": 2,
                            "content": "run 2 content",
                            "created_at": "2026-02-09T10:00:00+00:00",
                        }
                    ],
                },
            )
        return CoreResponse(request_id="rx", ok=True, result={})

    client.request = mock_request
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.get_task("T1", include_logs=True)

    assert result["task_id"] == "T1"
    assert result["logs"] == [
        {
            "run": 2,
            "content": "run 2 content",
            "created_at": "2026-02-09T10:00:00+00:00",
        }
    ]
    assert calls == [("tasks", "get"), ("tasks", "logs")]


@pytest.mark.asyncio
async def test_get_task_with_logs_fallback_when_query_unavailable() -> None:
    """get_task(include_logs=True) should return [] if tasks.logs is unavailable."""
    client = AsyncMock()
    calls: list[tuple[str, str]] = []

    async def mock_request(
        *,
        session_id,
        session_profile,
        session_origin,
        capability,
        method,
        params,
    ):
        calls.append((capability, method))
        assert session_id == SESSION
        assert session_profile is None
        assert session_origin is None
        if capability == "tasks" and method == "get":
            return CoreResponse(
                request_id="r1",
                ok=True,
                result={
                    "found": True,
                    "task": {
                        "id": "T1",
                        "title": "T",
                        "status": "backlog",
                        "description": None,
                        "acceptance_criteria": [],
                    },
                },
            )
        if capability == "tasks" and method == "logs":
            return CoreResponse(
                request_id="r2",
                ok=False,
                error=CoreErrorDetail(code="UNKNOWN_METHOD", message="No handler"),
            )
        return CoreResponse(request_id="rx", ok=True, result={})

    client.request = mock_request
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.get_task("T1", include_logs=True)

    assert result["task_id"] == "T1"
    assert result["status"] == "backlog"
    assert result["logs"] == []
    assert calls == [("tasks", "get"), ("tasks", "logs")]


@pytest.mark.asyncio
async def test_get_task_summary_mode_truncates_large_fields() -> None:
    """get_task(mode=summary) trims oversized scratchpad/log payloads."""
    client = AsyncMock()

    async def mock_request(
        *,
        session_id,
        session_profile,
        session_origin,
        capability,
        method,
        params,
    ):
        assert session_id == SESSION
        assert session_profile is None
        assert session_origin is None
        if capability == "tasks" and method == "get":
            return CoreResponse(
                request_id="r1",
                ok=True,
                result={
                    "found": True,
                    "task": {
                        "id": "T1",
                        "title": "T",
                        "status": "backlog",
                        "description": None,
                        "acceptance_criteria": [],
                    },
                },
            )
        if capability == "tasks" and method == "scratchpad":
            return CoreResponse(
                request_id="r2",
                ok=True,
                result={"content": "s" * 12_000},
            )
        if capability == "tasks" and method == "logs":
            return CoreResponse(
                request_id="r3",
                ok=True,
                result={
                    "task_id": "T1",
                    "logs": [
                        {
                            "run": idx,
                            "content": f"log-{idx}-" + ("x" * 4_000),
                            "created_at": "2026-02-09T10:00:00+00:00",
                        }
                        for idx in range(1, 6)
                    ],
                },
            )
        return CoreResponse(request_id="rx", ok=True, result={})

    client.request = mock_request
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.get_task(
        "T1",
        include_scratchpad=True,
        include_logs=True,
        mode="summary",
    )

    assert "[truncated " in result["scratchpad"]
    assert len(result["logs"]) == 3
    assert [entry["run"] for entry in result["logs"]] == [3, 4, 5]
    assert all("[truncated " in entry["content"] for entry in result["logs"])


@pytest.mark.asyncio
async def test_get_task_summary_mode_truncates_description_and_acceptance_criteria() -> None:
    """get_task(mode=summary) trims oversized description and criteria lists."""
    client = make_client(
        {
            "found": True,
            "task": {
                "id": "T-TRUNC",
                "title": "Big task",
                "status": "backlog",
                "description": "d" * 8_000,
                "acceptance_criteria": [f"criterion-{idx}-" + ("x" * 800) for idx in range(30)],
            },
        }
    )
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.get_task("T-TRUNC", mode="summary")

    assert "[truncated " in (result["description"] or "")
    assert isinstance(result["acceptance_criteria"], list)
    assert len(result["acceptance_criteria"]) == 21
    assert result["acceptance_criteria"][-1].startswith("[truncated ")


@pytest.mark.asyncio
async def test_list_tasks_with_coordination_filters() -> None:
    """list_tasks should pass filter/exclusion/scratchpad flags to tasks.list."""
    client = make_client(
        {
            "tasks": [
                {"id": "T1", "title": "Task 1", "status": "IN_PROGRESS"},
                {"id": "T2", "title": "Task 2", "status": "IN_PROGRESS"},
            ],
            "count": 2,
        }
    )
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.list_tasks(
        filter="IN_PROGRESS",
        exclude_task_ids=["T3"],
        include_scratchpad=True,
    )

    assert len(result["tasks"]) == 2
    assert result["tasks"][0]["id"] == "T1"
    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="tasks",
        method="list",
        params={
            "filter": "IN_PROGRESS",
            "exclude_task_ids": ["T3"],
            "include_scratchpad": True,
        },
    )


@pytest.mark.asyncio
async def test_update_scratchpad() -> None:
    """update_scratchpad should translate to tasks.update_scratchpad command."""
    client = make_client({"success": True, "task_id": "T1", "message": "updated"})
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.update_scratchpad("T1", "new notes")

    assert result["success"] is True
    assert result["task_id"] == "T1"
    assert result["message"] == "updated"
    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="tasks",
        method="update_scratchpad",
        params={"task_id": "T1", "content": "new notes"},
    )


@pytest.mark.asyncio
async def test_create_task_accepts_extended_fields() -> None:
    client = make_client({"success": True, "task_id": "T1"})
    bridge = CoreClientBridge(client, SESSION)
    await bridge.create_task(
        title="New task",
        description="desc",
        project_id="proj-1",
        status="IN_PROGRESS",
        priority="HIGH",
        task_type="AUTO",
        terminal_backend="tmux",
        agent_backend="codex",
        parent_id="parent-1",
        base_branch="develop",
        acceptance_criteria=["a", "b"],
        created_by="agent-1",
    )

    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="tasks",
        method="create",
        params={
            "title": "New task",
            "description": "desc",
            "project_id": "proj-1",
            "status": "IN_PROGRESS",
            "priority": "HIGH",
            "task_type": "AUTO",
            "terminal_backend": "tmux",
            "agent_backend": "codex",
            "parent_id": "parent-1",
            "base_branch": "develop",
            "acceptance_criteria": ["a", "b"],
            "created_by": "agent-1",
        },
    )


@pytest.mark.asyncio
async def test_create_task_accepts_scalar_acceptance_criteria() -> None:
    client = make_client({"success": True, "task_id": "T1"})
    bridge = CoreClientBridge(client, SESSION)
    await bridge.create_task(
        title="New task",
        acceptance_criteria="single criterion",
    )

    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="tasks",
        method="create",
        params={
            "title": "New task",
            "description": "",
            "acceptance_criteria": "single criterion",
        },
    )


@pytest.mark.asyncio
async def test_update_task_accepts_extended_fields() -> None:
    client = make_client({"success": True, "task_id": "T1"})
    bridge = CoreClientBridge(client, SESSION)
    await bridge.update_task(
        "T1",
        task_type="PAIR",
        status="BACKLOG",
        priority="MEDIUM",
        terminal_backend="cursor",
        agent_backend="claude",
        parent_id="parent-1",
        project_id="proj-1",
        base_branch="main",
        acceptance_criteria=["done"],
    )

    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="tasks",
        method="update",
        params={
            "task_id": "T1",
            "task_type": "PAIR",
            "status": "BACKLOG",
            "priority": "MEDIUM",
            "terminal_backend": "cursor",
            "agent_backend": "claude",
            "parent_id": "parent-1",
            "project_id": "proj-1",
            "base_branch": "main",
            "acceptance_criteria": ["done"],
        },
    )


@pytest.mark.asyncio
async def test_bridge_error_uses_fallback_message_when_core_message_empty() -> None:
    client = AsyncMock()

    async def mock_request(
        *,
        session_id,
        session_profile,
        session_origin,
        capability,
        method,
        params,
    ):
        del session_id, session_profile, session_origin, capability, method, params
        return CoreResponse(
            request_id="r1",
            ok=False,
            error=CoreErrorDetail(code="INVALID_PARAMS", message=""),
        )

    client.request = mock_request
    bridge = CoreClientBridge(client, SESSION)

    with pytest.raises(MCPBridgeError) as exc_info:
        await bridge.get_context("TASK-001")

    assert exc_info.value.code == "INVALID_PARAMS"
    assert exc_info.value.message == "tasks.context request failed"
