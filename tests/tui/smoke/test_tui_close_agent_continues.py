"""Feature tests: TUI client disconnect does not disrupt other IPC clients."""

from __future__ import annotations

import shutil
import sys
import tempfile
from pathlib import Path

import pytest

from kagan.core.ipc.client import IPCClient
from kagan.core.ipc.contracts import CoreRequest, CoreResponse
from kagan.core.ipc.discovery import CoreEndpoint
from kagan.core.ipc.server import IPCServer
from kagan.core.ipc.transports import UnixSocketTransport

# ---------------------------------------------------------------------------
# Shared state handler
# ---------------------------------------------------------------------------

_state: dict[str, str] = {}


async def _stateful_handler(req: CoreRequest) -> CoreResponse:
    """Handler that persists key-value state across clients."""
    if req.method == "set":
        key = req.params.get("key", "")
        value = req.params.get("value", "")
        _state[key] = value
        return CoreResponse.success(req.request_id, result={"set": True})

    if req.method == "get":
        key = req.params.get("key", "")
        return CoreResponse.success(
            req.request_id,
            result={"value": _state.get(key)},
        )

    return CoreResponse.failure(req.request_id, code="UNKNOWN_METHOD", message="unknown")


@pytest.fixture
def short_tmp():  # type: ignore[override]
    """Create a short temp directory for Unix socket paths (macOS 104-byte limit)."""
    d = tempfile.mkdtemp(prefix="k-", dir="/tmp")
    yield Path(d)
    shutil.rmtree(d, ignore_errors=True)


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
async def test_server_survives_client_disconnect(short_tmp) -> None:
    """When a TUI-style client disconnects, the server keeps running and serves others."""
    _state.clear()
    sock = str(short_tmp / "t.sock")
    transport = UnixSocketTransport(path=sock)
    server = IPCServer(handler=_stateful_handler, transport=transport)
    await server.start()

    ep = CoreEndpoint(transport="socket", address=sock, token=server.token)

    # Client A ("TUI") writes some state then disconnects
    client_a = IPCClient(ep, transport=UnixSocketTransport(path=sock))
    await client_a.connect()
    resp = await client_a.request(
        session_id="tui",
        capability="state",
        method="set",
        params={"key": "progress", "value": "50%"},
    )
    assert resp.ok
    await client_a.close()
    assert not client_a.is_connected

    # Server should still be running
    assert server.is_running

    # Client B ("MCP") connects and can see the state written by Client A
    client_b = IPCClient(ep, transport=UnixSocketTransport(path=sock))
    await client_b.connect()
    resp = await client_b.request(
        session_id="mcp",
        capability="state",
        method="get",
        params={"key": "progress"},
    )
    assert resp.ok
    assert resp.result is not None
    assert resp.result["value"] == "50%"

    await client_b.close()
    await server.stop()


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
async def test_state_mutations_visible_across_clients(short_tmp) -> None:
    """State changes from one client are visible to another client."""
    _state.clear()
    sock = str(short_tmp / "t.sock")
    transport = UnixSocketTransport(path=sock)
    server = IPCServer(handler=_stateful_handler, transport=transport)
    await server.start()

    ep = CoreEndpoint(transport="socket", address=sock, token=server.token)
    client_a = IPCClient(ep, transport=UnixSocketTransport(path=sock))
    client_b = IPCClient(ep, transport=UnixSocketTransport(path=sock))
    await client_a.connect()
    await client_b.connect()

    try:
        # Client A sets a value
        await client_a.request(
            session_id="tui",
            capability="state",
            method="set",
            params={"key": "status", "value": "in_progress"},
        )

        # Client B reads the value
        resp = await client_b.request(
            session_id="mcp",
            capability="state",
            method="get",
            params={"key": "status"},
        )
        assert resp.ok
        assert resp.result is not None
        assert resp.result["value"] == "in_progress"

        # Client B updates the value
        await client_b.request(
            session_id="mcp",
            capability="state",
            method="set",
            params={"key": "status", "value": "review"},
        )

        # Client A sees the updated value
        resp = await client_a.request(
            session_id="tui",
            capability="state",
            method="get",
            params={"key": "status"},
        )
        assert resp.ok
        assert resp.result is not None
        assert resp.result["value"] == "review"
    finally:
        await client_a.close()
        await client_b.close()
        await server.stop()


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
async def test_reconnect_after_disconnect(short_tmp) -> None:
    """A client can reconnect after disconnecting and resume communication."""
    _state.clear()
    sock = str(short_tmp / "t.sock")
    transport = UnixSocketTransport(path=sock)
    server = IPCServer(handler=_stateful_handler, transport=transport)
    await server.start()

    ep = CoreEndpoint(transport="socket", address=sock, token=server.token)
    client = IPCClient(ep, transport=UnixSocketTransport(path=sock))

    try:
        # First session: set state
        await client.connect()
        await client.request(
            session_id="tui",
            capability="state",
            method="set",
            params={"key": "round", "value": "1"},
        )
        await client.close()

        # Reconnect
        client = IPCClient(ep, transport=UnixSocketTransport(path=sock))
        await client.connect()
        resp = await client.request(
            session_id="tui",
            capability="state",
            method="get",
            params={"key": "round"},
        )
        assert resp.ok
        assert resp.result is not None
        assert resp.result["value"] == "1"
    finally:
        await client.close()
        await server.stop()
