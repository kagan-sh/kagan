"""Operational and error-path tests for CoreClientBridge."""

from __future__ import annotations

from unittest.mock import AsyncMock, patch

import pytest
from tests.mcp.contract._bridge_test_support import SESSION, make_client

from kagan.core.ipc.contracts import CoreErrorDetail, CoreResponse
from kagan.mcp.tools import CoreClientBridge, MCPBridgeError


@pytest.mark.asyncio
async def test_request_review_success() -> None:
    """request_review should translate to review.request command."""
    client = make_client({"success": True, "task_id": "T1", "status": "review"})
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.request_review("T1", "Done implementing")

    assert result["status"] == "review"
    assert result["message"] == "Ready for merge"
    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="review",
        method="request",
        params={"task_id": "T1", "summary": "Done implementing"},
    )


@pytest.mark.asyncio
async def test_request_review_failure() -> None:
    """request_review should return error when core reports failure."""
    client = make_client({"success": False, "message": "Task not found"})
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.request_review("T1", "summary")

    assert result["status"] == "error"
    assert "Task not found" in result["message"]
    assert result["success"] is False


@pytest.mark.asyncio
async def test_request_review_preserves_recovery_fields() -> None:
    """request_review should preserve next_tool guidance from core response."""
    client = make_client(
        {
            "success": False,
            "message": "Review blocked",
            "code": "REVIEW_BLOCKED",
            "hint": "Resolve overlap and retry.",
            "next_tool": "task_patch",
            "next_arguments": {"task_id": "T1", "status": "IN_PROGRESS"},
        }
    )
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.request_review("T1", "summary")

    assert result["code"] == "REVIEW_BLOCKED"
    assert result["hint"] == "Resolve overlap and retry."
    assert result["next_tool"] == "task_patch"
    assert result["next_arguments"] == {"task_id": "T1", "status": "IN_PROGRESS"}


@pytest.mark.asyncio
@pytest.mark.parametrize(
    ("bridge_method", "kwargs", "expected_method", "expected_params"),
    [
        (
            "submit_job",
            {"task_id": "T1", "action": "agent_start"},
            "submit",
            {"task_id": "T1", "action": "agent_start"},
        ),
        (
            "get_job",
            {"job_id": "J1", "task_id": "T1"},
            "get",
            {"job_id": "J1", "task_id": "T1"},
        ),
        (
            "wait_job",
            {"job_id": "J1", "task_id": "T1", "timeout_seconds": 0.25},
            "wait",
            {"job_id": "J1", "task_id": "T1", "timeout_seconds": 0.25},
        ),
        (
            "list_job_events",
            {"job_id": "J1", "task_id": "T1", "limit": 25, "offset": 10},
            "events",
            {"job_id": "J1", "task_id": "T1", "limit": 25, "offset": 10},
        ),
        (
            "cancel_job",
            {"job_id": "J1", "task_id": "T1"},
            "cancel",
            {"job_id": "J1", "task_id": "T1"},
        ),
    ],
)
async def test_job_methods_route_to_core_requests(
    bridge_method: str,
    kwargs: dict[str, object],
    expected_method: str,
    expected_params: dict[str, object],
) -> None:
    payload = {"success": True, "task_id": "T1", "job_id": "J1", "status": "running"}
    if bridge_method == "list_job_events":
        payload = {"success": True, "task_id": "T1", "job_id": "J1", "events": []}

    client = make_client(payload)
    bridge = CoreClientBridge(client, SESSION)
    method = getattr(bridge, bridge_method)
    result = await method(**kwargs)

    assert result["job_id"] == "J1"
    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="jobs",
        method=expected_method,
        params=expected_params,
    )


@pytest.mark.asyncio
async def test_wait_job_preserves_timeout_metadata() -> None:
    payload = {
        "success": True,
        "task_id": "T1",
        "job_id": "J1",
        "status": "running",
        "timed_out": True,
        "timeout": {"requested_seconds": 0.25, "waited_seconds": 0.25},
    }
    client = make_client(payload)
    bridge = CoreClientBridge(client, SESSION)

    result = await bridge.wait_job(job_id="J1", task_id="T1", timeout_seconds=0.25)

    assert result["timed_out"] is True
    assert result["timeout"] == {"requested_seconds": 0.25, "waited_seconds": 0.25}


@pytest.mark.asyncio
async def test_create_session() -> None:
    """create_session should translate to sessions.create command."""
    client = make_client(
        {"success": True, "task_id": "T1", "session_name": "kagan-T1", "backend": "tmux"}
    )
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.create_session("T1", reuse_if_exists=False, worktree_path="/tmp/wt")

    assert result["success"] is True
    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="sessions",
        method="create",
        params={
            "task_id": "T1",
            "reuse_if_exists": False,
            "worktree_path": "/tmp/wt",
        },
    )


@pytest.mark.asyncio
async def test_session_exists() -> None:
    """session_exists should translate to sessions.exists command."""
    client = make_client({"task_id": "T1", "exists": True, "session_name": "kagan-T1"})
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.session_exists("T1")

    assert result["exists"] is True
    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="sessions",
        method="exists",
        params={"task_id": "T1"},
    )


@pytest.mark.asyncio
async def test_kill_session() -> None:
    """kill_session should translate to sessions.kill command."""
    client = make_client({"success": True, "task_id": "T1"})
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.kill_session("T1")

    assert result["success"] is True
    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="sessions",
        method="kill",
        params={"task_id": "T1"},
    )


@pytest.mark.asyncio
async def test_get_settings() -> None:
    """get_settings should translate to settings.get query."""
    client = make_client({"settings": {"general.auto_review": True}})
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.get_settings()

    assert result["settings"]["general.auto_review"] is True
    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="settings",
        method="get",
        params={},
    )


@pytest.mark.asyncio
async def test_update_settings() -> None:
    """update_settings should translate to settings.update command."""
    client = make_client({"success": True, "updated": {"general.auto_review": False}})
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.update_settings({"general.auto_review": False})

    assert result["success"] is True
    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="settings",
        method="update",
        params={"fields": {"general.auto_review": False}},
    )


@pytest.mark.asyncio
async def test_bridge_reconnects_and_retries_after_auth_failure() -> None:
    """Bridge should refresh endpoint and retry once after AUTH_FAILED."""
    stale_client = AsyncMock()
    stale_client.connect = AsyncMock()
    stale_client.close = AsyncMock()
    stale_client.is_connected = True
    stale_client.request = AsyncMock(
        return_value=CoreResponse(
            request_id="r1",
            ok=False,
            error=CoreErrorDetail(code="AUTH_FAILED", message="Invalid or missing bearer token"),
        )
    )

    fresh_client = AsyncMock()
    fresh_client.connect = AsyncMock()
    fresh_client.close = AsyncMock()
    fresh_client.is_connected = True
    fresh_client.request = AsyncMock(
        return_value=CoreResponse(
            request_id="r2",
            ok=True,
            result={"settings": {"general.auto_review": True}},
        )
    )

    with (
        patch(
            "kagan.core.ipc.discovery.discover_core_endpoint",
            return_value=object(),
        ),
        patch("kagan.core.ipc.client.IPCClient", return_value=fresh_client),
    ):
        bridge = CoreClientBridge(stale_client, SESSION)
        result = await bridge.get_settings()

    assert result["settings"]["general.auto_review"] is True
    assert stale_client.request.await_count == 1
    assert stale_client.close.await_count == 1
    assert fresh_client.connect.await_count == 1
    assert fresh_client.request.await_count == 1


@pytest.mark.asyncio
async def test_bridge_reconnects_and_retries_after_connection_error() -> None:
    """Bridge should reconnect existing client and retry after transport failure."""
    client = AsyncMock()
    client.connect = AsyncMock(return_value=None)
    client.close = AsyncMock()
    client.is_connected = True
    client.request = AsyncMock(
        side_effect=[
            ConnectionError("broken pipe"),
            CoreResponse(
                request_id="r2",
                ok=True,
                result={"settings": {"general.auto_review": True}},
            ),
        ]
    )

    with patch("kagan.core.ipc.discovery.discover_core_endpoint") as discover_mock:
        bridge = CoreClientBridge(client, SESSION)
        result = await bridge.get_settings()

    assert result["settings"]["general.auto_review"] is True
    assert client.connect.await_count == 1
    assert client.request.await_count == 2
    discover_mock.assert_not_called()


@pytest.mark.asyncio
async def test_bridge_refreshes_endpoint_when_reconnect_fails_on_connection_error() -> None:
    """Bridge should refresh endpoint when reconnecting current client fails."""
    stale_client = AsyncMock()
    stale_client.connect = AsyncMock(side_effect=ConnectionError("still down"))
    stale_client.close = AsyncMock()
    stale_client.is_connected = False
    stale_client.request = AsyncMock(side_effect=ConnectionError("broken pipe"))

    fresh_client = AsyncMock()
    fresh_client.connect = AsyncMock(return_value=None)
    fresh_client.close = AsyncMock()
    fresh_client.is_connected = True
    fresh_client.request = AsyncMock(
        return_value=CoreResponse(
            request_id="r3",
            ok=True,
            result={"settings": {"general.auto_review": True}},
        )
    )

    with (
        patch("kagan.core.ipc.discovery.discover_core_endpoint", return_value=object()),
        patch("kagan.core.ipc.client.IPCClient", return_value=fresh_client),
    ):
        bridge = CoreClientBridge(stale_client, SESSION)
        result = await bridge.get_settings()

    assert result["settings"]["general.auto_review"] is True
    assert stale_client.request.await_count == 1
    assert stale_client.connect.await_count == 1
    assert stale_client.close.await_count == 1
    assert fresh_client.connect.await_count == 1
    assert fresh_client.request.await_count == 1


@pytest.mark.asyncio
async def test_get_scratchpad() -> None:
    """get_scratchpad should translate to tasks.scratchpad query."""
    client = make_client({"task_id": "T1", "content": "my notes"})
    bridge = CoreClientBridge(client, SESSION)
    result = await bridge.get_scratchpad("T1")

    assert result == "my notes"
    client.request.assert_called_once_with(
        session_id=SESSION,
        session_profile=None,
        session_origin=None,
        capability="tasks",
        method="scratchpad",
        params={"task_id": "T1"},
    )


@pytest.mark.asyncio
async def test_core_error_raises_typed_bridge_error() -> None:
    """Bridge should raise MCPBridgeError with normalized code/message."""
    client = make_client(ok=False)
    bridge = CoreClientBridge(client, SESSION)

    with pytest.raises(MCPBridgeError) as exc_info:
        await bridge.get_task("T1")
    assert exc_info.value.code == "ERR"
    assert exc_info.value.kind == "query"
    assert exc_info.value.capability == "tasks"
    assert exc_info.value.method == "get"
    assert "Core query tasks.get failed [ERR]" in str(exc_info.value)


@pytest.mark.asyncio
async def test_auth_failed_normalizes_to_stale_token_code_when_recovery_fails() -> None:
    stale_client = AsyncMock()
    stale_client.connect = AsyncMock()
    stale_client.close = AsyncMock()
    stale_client.is_connected = True
    stale_client.request = AsyncMock(
        return_value=CoreResponse(
            request_id="r1",
            ok=False,
            error=CoreErrorDetail(code="AUTH_FAILED", message="Invalid token"),
        )
    )

    with patch("kagan.core.ipc.discovery.discover_core_endpoint", return_value=None):
        bridge = CoreClientBridge(stale_client, SESSION)
        with pytest.raises(MCPBridgeError) as exc_info:
            await bridge.get_settings()

    assert exc_info.value.code == "AUTH_STALE_TOKEN"
    assert "stale" in exc_info.value.message.lower()
