"""MCP contract tests for task_wait long-poll primitive."""

from __future__ import annotations

from typing import Any
from unittest.mock import AsyncMock

from kagan.core.ipc.contracts import CoreResponse
from kagan.mcp.tools import CoreClientBridge


def _bridge_for_routes(
    routes: dict[tuple[str, str], dict[str, Any]],
    *,
    captured_calls: list[dict[str, Any]] | None = None,
) -> CoreClientBridge:
    client = AsyncMock()
    client.is_connected = True

    async def _request(
        *,
        session_id: str,
        session_profile: str | None = None,
        session_origin: str | None = None,
        capability: str,
        method: str,
        params: dict[str, Any] | None = None,
        idempotency_key: str | None = None,
        request_timeout_seconds: float | None = None,
    ) -> CoreResponse:
        del session_id, session_profile, session_origin, idempotency_key
        if captured_calls is not None:
            captured_calls.append(
                {
                    "capability": capability,
                    "method": method,
                    "params": params or {},
                    "request_timeout_seconds": request_timeout_seconds,
                }
            )
        payload = routes[(capability, method)]
        return CoreResponse.success("req-1", result=payload)

    client.request = _request
    return CoreClientBridge(client, session_id="test-session")


async def test_wait_task_sends_correct_params() -> None:
    """Bridge sends task_id and optional params to tasks.wait."""
    calls: list[dict[str, Any]] = []
    bridge = _bridge_for_routes(
        {
            ("tasks", "wait"): {
                "changed": True,
                "timed_out": False,
                "task_id": "T-100",
                "previous_status": "BACKLOG",
                "current_status": "IN_PROGRESS",
                "changed_at": "2026-02-13T10:00:00+00:00",
                "task": {
                    "id": "T-100",
                    "title": "Test",
                    "status": "IN_PROGRESS",
                },
                "code": "TASK_CHANGED",
                "message": "Task status changed",
            },
        },
        captured_calls=calls,
    )

    result = await bridge.wait_task(
        "T-100",
        timeout_seconds=30.0,
        wait_for_status=["IN_PROGRESS", "REVIEW"],
        from_updated_at="2026-02-13T09:00:00+00:00",
    )

    assert result["changed"] is True
    assert result["timed_out"] is False
    assert result["current_status"] == "IN_PROGRESS"
    assert calls == [
        {
            "capability": "tasks",
            "method": "wait",
            "params": {
                "task_id": "T-100",
                "timeout_seconds": 30.0,
                "wait_for_status": ["IN_PROGRESS", "REVIEW"],
                "from_updated_at": "2026-02-13T09:00:00+00:00",
            },
            "request_timeout_seconds": 35.0,
        }
    ]


async def test_wait_task_minimal_params() -> None:
    """Bridge sends only task_id when no optional params are provided."""
    calls: list[dict[str, Any]] = []
    bridge = _bridge_for_routes(
        {
            ("tasks", "wait"): {
                "changed": False,
                "timed_out": True,
                "task_id": "T-200",
                "code": "WAIT_TIMEOUT",
                "message": "No change detected",
            },
        },
        captured_calls=calls,
    )

    result = await bridge.wait_task("T-200")

    assert result["timed_out"] is True
    assert result["changed"] is False
    assert calls == [
        {
            "capability": "tasks",
            "method": "wait",
            "params": {"task_id": "T-200"},
            "request_timeout_seconds": 3605.0,
        }
    ]


async def test_wait_task_timeout_response() -> None:
    """Bridge correctly returns timeout response from core."""
    bridge = _bridge_for_routes(
        {
            ("tasks", "wait"): {
                "changed": False,
                "timed_out": True,
                "task_id": "T-300",
                "previous_status": "BACKLOG",
                "current_status": "BACKLOG",
                "changed_at": None,
                "task": None,
                "code": "WAIT_TIMEOUT",
                "message": "No change detected within 30s",
            },
        },
    )

    result = await bridge.wait_task("T-300", timeout_seconds=30.0)
    assert result["timed_out"] is True
    assert result["changed"] is False
    assert result["code"] == "WAIT_TIMEOUT"
    assert result["task"] is None


async def test_wait_task_already_at_status() -> None:
    """Bridge returns immediate result when task is already at target status."""
    bridge = _bridge_for_routes(
        {
            ("tasks", "wait"): {
                "changed": True,
                "timed_out": False,
                "task_id": "T-400",
                "previous_status": "REVIEW",
                "current_status": "REVIEW",
                "changed_at": "2026-02-13T10:00:00+00:00",
                "task": {"id": "T-400", "title": "Test", "status": "REVIEW"},
                "code": "ALREADY_AT_STATUS",
                "message": "Task already at target status REVIEW",
            },
        },
    )

    result = await bridge.wait_task(
        "T-400",
        wait_for_status=["REVIEW"],
    )
    assert result["changed"] is True
    assert result["code"] == "ALREADY_AT_STATUS"


async def test_wait_task_interrupted_response() -> None:
    """Bridge returns WAIT_INTERRUPTED on cancellation."""
    bridge = _bridge_for_routes(
        {
            ("tasks", "wait"): {
                "changed": False,
                "timed_out": False,
                "task_id": "T-500",
                "code": "WAIT_INTERRUPTED",
                "message": "Wait was interrupted",
            },
        },
    )

    result = await bridge.wait_task("T-500")
    assert result["changed"] is False
    assert result["code"] == "WAIT_INTERRUPTED"


async def test_wait_task_race_safe_cursor() -> None:
    """Bridge correctly passes from_updated_at for race-safe resume."""
    calls: list[dict[str, Any]] = []
    bridge = _bridge_for_routes(
        {
            ("tasks", "wait"): {
                "changed": True,
                "timed_out": False,
                "task_id": "T-600",
                "code": "CHANGED_SINCE_CURSOR",
                "message": "Task changed since from_updated_at cursor",
                "current_status": "IN_PROGRESS",
                "task": {"id": "T-600", "status": "IN_PROGRESS"},
            },
        },
        captured_calls=calls,
    )

    result = await bridge.wait_task(
        "T-600",
        from_updated_at="2026-02-13T08:00:00+00:00",
    )
    assert result["changed"] is True
    assert result["code"] == "CHANGED_SINCE_CURSOR"
    assert calls[0]["params"]["from_updated_at"] == "2026-02-13T08:00:00+00:00"


async def test_wait_task_accepts_string_timeout_and_status_filter() -> None:
    """Bridge forwards string-based wait params for tolerant server parsing."""
    calls: list[dict[str, Any]] = []
    bridge = _bridge_for_routes(
        {
            ("tasks", "wait"): {
                "changed": False,
                "timed_out": True,
                "task_id": "T-700",
                "code": "WAIT_TIMEOUT",
                "message": "No change detected within 30s",
            },
        },
        captured_calls=calls,
    )

    result = await bridge.wait_task(
        "T-700",
        timeout_seconds="30",
        wait_for_status="REVIEW,DONE",
    )

    assert result["timed_out"] is True
    assert calls == [
        {
            "capability": "tasks",
            "method": "wait",
            "params": {
                "task_id": "T-700",
                "timeout_seconds": 30.0,
                "wait_for_status": "REVIEW,DONE",
            },
            "request_timeout_seconds": 35.0,
        }
    ]
