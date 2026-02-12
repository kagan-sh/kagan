"""Tests for MCP server startup behavior when core is unavailable."""

from __future__ import annotations

from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from kagan.core.ipc.discovery import CoreEndpoint
from kagan.mcp import server as mcp_server
from kagan.mcp.server import MCPRuntimeConfig, MCPStartupError, _create_mcp_server, _mcp_lifespan


def _tool_names(mcp: object) -> set[str]:
    tool_manager = mcp._tool_manager  # type: ignore[attr-defined]  # quality-allow-private
    return set(tool_manager._tools.keys())  # type: ignore[attr-defined]  # quality-allow-private


@pytest.mark.asyncio
async def test_no_endpoint_raises_structured_startup_error() -> None:
    """Lifespan should fail fast when no core endpoint is discovered."""
    with patch("kagan.mcp.server._resolve_or_autostart_endpoint", new=AsyncMock(return_value=None)):
        with pytest.raises(MCPStartupError) as exc_info:
            async with _mcp_lifespan(_create_mcp_server()):
                pass

    assert exc_info.value.as_dict() == {
        "code": "NO_ENDPOINT",
        "message": "No active Kagan core endpoint was discovered.",
        "hint": "Start Kagan or run `kagan core start`, then reconnect MCP.",
    }
    assert str(exc_info.value) == (
        "[NO_ENDPOINT] No active Kagan core endpoint was discovered. "
        "Hint: Start Kagan or run `kagan core start`, then reconnect MCP."
    )


@pytest.mark.asyncio
async def test_connection_failure_raises_structured_startup_error() -> None:
    """Lifespan should fail fast when IPC connection fails."""
    fake_endpoint = MagicMock()
    fake_endpoint.transport = "tcp"
    fake_endpoint.address = "127.0.0.1"
    fake_endpoint.port = 9999
    fake_endpoint.token = "test-token"

    mock_client = AsyncMock()
    mock_client.connect = AsyncMock(side_effect=ConnectionError("refused"))
    mock_client.is_connected = False

    with (
        patch(
            "kagan.mcp.server._resolve_or_autostart_endpoint",
            new=AsyncMock(return_value=fake_endpoint),
        ),
        patch("kagan.mcp.server.IPCClient", return_value=mock_client),
    ):
        with pytest.raises(MCPStartupError) as exc_info:
            async with _mcp_lifespan(_create_mcp_server()):
                pass

    assert exc_info.value.code == "DISCONNECTED"
    assert exc_info.value.hint == "Ensure core is running and reachable, then reconnect MCP."
    assert exc_info.value.message == "Kagan core is unreachable at tcp://127.0.0.1:9999: refused"


@pytest.mark.asyncio
async def test_resolve_or_autostart_endpoint_starts_core_when_enabled() -> None:
    """MCP auto-starts core when discovery fails and autostart is enabled."""
    expected = CoreEndpoint(
        transport="tcp",
        address="127.0.0.1",
        port=45678,
        token="token",
    )
    mock_start = AsyncMock(return_value=expected)

    with (
        patch.object(mcp_server, "_resolve_endpoint", return_value=None),
        patch.object(mcp_server, "_is_core_autostart_enabled", return_value=True),
        patch.object(mcp_server, "ensure_core_running", new=mock_start),
    ):
        endpoint = await mcp_server._resolve_or_autostart_endpoint(MCPRuntimeConfig())

    assert endpoint == expected
    mock_start.assert_awaited_once()


@pytest.mark.asyncio
async def test_resolve_or_autostart_endpoint_skips_autostart_for_explicit_override() -> None:
    """When endpoint override is explicit, MCP should not auto-start core."""
    mock_start = AsyncMock()
    runtime_config = MCPRuntimeConfig(endpoint="tcp://127.0.0.1:45678")

    with (
        patch.object(mcp_server, "_resolve_endpoint", return_value=None),
        patch.object(mcp_server, "_is_core_autostart_enabled", return_value=True),
        patch.object(mcp_server, "ensure_core_running", new=mock_start),
    ):
        endpoint = await mcp_server._resolve_or_autostart_endpoint(runtime_config)

    assert endpoint is None
    mock_start.assert_not_awaited()


def test_resolve_endpoint_keeps_unix_socket_override() -> None:
    """A bare socket path override resolves as socket transport, not TCP."""
    with (
        patch.object(mcp_server, "discover_core_endpoint", return_value=None),
        patch.object(mcp_server, "_read_local_core_token", return_value="token"),
    ):
        endpoint = mcp_server._resolve_endpoint(MCPRuntimeConfig(endpoint="/tmp/kagan-core.sock"))
    assert endpoint == CoreEndpoint(
        transport="socket",
        address="/tmp/kagan-core.sock",
        port=None,
        pid=None,
        token="token",
    )


def test_resolve_endpoint_parses_tcp_host_port_override() -> None:
    """A host:port override resolves as TCP transport with parsed port."""
    with (
        patch.object(mcp_server, "discover_core_endpoint", return_value=None),
        patch.object(mcp_server, "_read_local_core_token", return_value="token"),
    ):
        endpoint = mcp_server._resolve_endpoint(MCPRuntimeConfig(endpoint="127.0.0.1:9876"))
    assert endpoint == CoreEndpoint(
        transport="tcp",
        address="127.0.0.1",
        port=9876,
        pid=None,
        token="token",
    )


def test_create_server_applies_runtime_capability_override() -> None:
    """Runtime capability override should scope registered tools."""
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(capability_profile="viewer", identity="kagan"),
    )
    names = _tool_names(mcp)

    assert "tasks_list" in names
    assert "tasks_update" not in names
    assert "request_review" not in names


def test_create_server_caps_profile_by_identity_ceiling() -> None:
    """kagan identity should cap maintainer override to pair_worker capabilities."""
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(capability_profile="maintainer", identity="kagan"),
    )
    names = _tool_names(mcp)

    assert "sessions_create" in names
    assert "request_review" in names
    assert "settings_update" not in names
    assert "projects_open" not in names


def test_create_server_keeps_maintainer_for_admin_identity() -> None:
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(capability_profile="maintainer", identity="kagan_admin"),
    )
    names = _tool_names(mcp)

    assert "settings_update" in names
    assert "projects_open" in names


@pytest.mark.asyncio
async def test_lifespan_uses_runtime_config_for_bridge_metadata() -> None:
    """Lifespan should pass explicit runtime config session/profile/origin into bridge."""
    fake_endpoint = CoreEndpoint(
        transport="tcp",
        address="127.0.0.1",
        port=4444,
        token="token",
    )
    mock_client = AsyncMock()
    mock_client.connect = AsyncMock(return_value=None)
    mock_client.close = AsyncMock(return_value=None)
    mock_client.is_connected = True

    runtime_config = MCPRuntimeConfig(
        session_id="session-x",
        capability_profile="pair_worker",
        identity="kagan",
    )

    with (
        patch.object(
            mcp_server,
            "_resolve_or_autostart_endpoint",
            new=AsyncMock(return_value=fake_endpoint),
        ),
        patch.object(mcp_server, "IPCClient", return_value=mock_client),
    ):
        async with _mcp_lifespan(_create_mcp_server(), runtime_config) as ctx:
            assert ctx.bridge is not None
            assert ctx.client is mock_client
            assert ctx.bridge._session_id == "session-x"  # type: ignore[attr-defined]  # quality-allow-private
            assert (
                ctx.bridge._capability_profile == "pair_worker"  # type: ignore[attr-defined]  # quality-allow-private
            )
            assert ctx.bridge._session_origin == "kagan"  # type: ignore[attr-defined]  # quality-allow-private


def test_main_exits_with_structured_startup_error_message() -> None:
    """main() should surface startup failures as deterministic actionable output."""

    class _MCPStub:
        def run(self, *, transport: str) -> None:
            assert transport == "stdio"
            raise MCPStartupError(
                code="NO_ENDPOINT",
                message="No active Kagan core endpoint was discovered.",
                hint="Start Kagan or run `kagan core start`, then reconnect MCP.",
            )

    with patch.object(mcp_server, "_create_mcp_server", return_value=_MCPStub()):
        with pytest.raises(SystemExit) as exc_info:
            mcp_server.main(readonly=True)

    assert str(exc_info.value) == (
        "[NO_ENDPOINT] No active Kagan core endpoint was discovered. "
        "Hint: Start Kagan or run `kagan core start`, then reconnect MCP."
    )
