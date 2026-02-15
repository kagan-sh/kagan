"""Integration tests: parallel IPC clients with authorization enforcement."""

from __future__ import annotations

import asyncio
import sys
import tempfile
from pathlib import Path

import pytest

from kagan.core.ipc.client import IPCClient
from kagan.core.ipc.contracts import CoreRequest, CoreResponse
from kagan.core.ipc.discovery import CoreEndpoint
from kagan.core.ipc.server import IPCServer
from kagan.core.ipc.transports import UnixSocketTransport
from kagan.core.security import AuthorizationError, AuthorizationPolicy, CapabilityProfile

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_echo_handler(
    session_profiles: dict[str, AuthorizationPolicy] | None = None,
):
    """Return an IPC handler that echoes params with optional auth enforcement."""
    profiles = session_profiles or {}

    async def handler(req: CoreRequest) -> CoreResponse:
        if req.session_id in profiles:
            policy = profiles[req.session_id]
            try:
                policy.enforce(req.capability, req.method)
            except AuthorizationError as exc:
                return CoreResponse.failure(req.request_id, code=exc.code, message=str(exc))
        return CoreResponse.success(
            req.request_id,
            result={"echo": True, "capability": req.capability, "method": req.method},
        )

    return handler


def _endpoint(socket_path: str, token: str) -> CoreEndpoint:
    return CoreEndpoint(transport="socket", address=socket_path, token=token)


@pytest.fixture
def short_tmp():  # type: ignore[override]
    """Create a short temp directory for Unix socket paths (macOS 104-byte limit)."""
    d = tempfile.mkdtemp(prefix="k-", dir="/tmp")
    yield Path(d)
    import shutil

    shutil.rmtree(d, ignore_errors=True)


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
async def test_two_clients_concurrent_requests(short_tmp) -> None:
    """Two IPCClients can send requests concurrently to one IPCServer."""
    sock = str(short_tmp / "t.sock")
    transport = UnixSocketTransport(path=sock)
    server = IPCServer(handler=_make_echo_handler(), transport=transport)
    await server.start()

    ep = _endpoint(sock, server.token)
    client_a = IPCClient(ep, transport=UnixSocketTransport(path=sock))
    client_b = IPCClient(ep, transport=UnixSocketTransport(path=sock))
    await client_a.connect()
    await client_b.connect()

    try:
        resp_a, resp_b = await asyncio.gather(
            client_a.request(session_id="tui", capability="tasks", method="list"),
            client_b.request(session_id="mcp", capability="projects", method="list"),
        )
        assert resp_a.ok and resp_a.result is not None
        assert resp_a.result["capability"] == "tasks"
        assert resp_b.ok and resp_b.result is not None
        assert resp_b.result["capability"] == "projects"
    finally:
        await client_a.close()
        await client_b.close()
        await server.stop()


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
async def test_viewer_denied_destructive_command(short_tmp) -> None:
    """A viewer-profile session is denied access to a mutating command."""
    sock = str(short_tmp / "t.sock")
    transport = UnixSocketTransport(path=sock)
    profiles: dict[str, AuthorizationPolicy] = {
        "viewer-session": AuthorizationPolicy(CapabilityProfile.VIEWER),
    }
    server = IPCServer(handler=_make_echo_handler(session_profiles=profiles), transport=transport)
    await server.start()

    ep = _endpoint(sock, server.token)
    client = IPCClient(ep, transport=UnixSocketTransport(path=sock))
    await client.connect()

    try:
        resp = await client.request(
            session_id="viewer-session",
            capability="tasks",
            method="delete",
        )
        assert not resp.ok
        assert resp.error is not None
        assert resp.error.code == "AUTHORIZATION_DENIED"
    finally:
        await client.close()
        await server.stop()


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
async def test_viewer_allowed_read_query(short_tmp) -> None:
    """A viewer-profile session can issue a read-only query."""
    sock = str(short_tmp / "t.sock")
    transport = UnixSocketTransport(path=sock)
    profiles: dict[str, AuthorizationPolicy] = {
        "viewer-session": AuthorizationPolicy(CapabilityProfile.VIEWER),
    }
    server = IPCServer(handler=_make_echo_handler(session_profiles=profiles), transport=transport)
    await server.start()

    ep = _endpoint(sock, server.token)
    client = IPCClient(ep, transport=UnixSocketTransport(path=sock))
    await client.connect()

    try:
        resp = await client.request(
            session_id="viewer-session",
            capability="tasks",
            method="list",
        )
        assert resp.ok
        assert resp.result is not None
        assert resp.result["echo"] is True
    finally:
        await client.close()
        await server.stop()


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
async def test_invalid_token_rejected(short_tmp) -> None:
    """A client with a wrong bearer token gets AUTH_FAILED."""
    sock = str(short_tmp / "t.sock")
    transport = UnixSocketTransport(path=sock)
    server = IPCServer(handler=_make_echo_handler(), transport=transport)
    await server.start()

    wrong_ep = _endpoint(sock, "wrong-token-value")
    client = IPCClient(wrong_ep, transport=UnixSocketTransport(path=sock))
    await client.connect()

    try:
        resp = await client.request(session_id="s1", capability="tasks", method="list")
        assert not resp.ok
        assert resp.error is not None
        assert resp.error.code == "AUTH_FAILED"
    finally:
        await client.close()
        await server.stop()


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
async def test_multiple_profiles_on_same_server(short_tmp) -> None:
    """Different sessions with different profiles get correct auth results."""
    sock = str(short_tmp / "t.sock")
    transport = UnixSocketTransport(path=sock)
    profiles: dict[str, AuthorizationPolicy] = {
        "viewer": AuthorizationPolicy(CapabilityProfile.VIEWER),
        "operator": AuthorizationPolicy(CapabilityProfile.OPERATOR),
    }
    server = IPCServer(handler=_make_echo_handler(session_profiles=profiles), transport=transport)
    await server.start()

    ep = _endpoint(sock, server.token)
    client = IPCClient(ep, transport=UnixSocketTransport(path=sock))
    await client.connect()

    try:
        # Viewer cannot delete
        resp_deny = await client.request(session_id="viewer", capability="tasks", method="delete")
        assert not resp_deny.ok

        # Operator can create
        resp_ok = await client.request(session_id="operator", capability="tasks", method="create")
        assert resp_ok.ok

        # Operator cannot delete (only maintainer can)
        resp_deny2 = await client.request(
            session_id="operator", capability="tasks", method="delete"
        )
        assert not resp_deny2.ok
    finally:
        await client.close()
        await server.stop()


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
async def test_client_rejects_mismatched_request_id(short_tmp) -> None:
    """Client closes connection when server returns a mismatched request_id."""
    sock = str(short_tmp / "t.sock")
    transport = UnixSocketTransport(path=sock)

    async def mismatch_handler(req: CoreRequest) -> CoreResponse:
        return CoreResponse.success("wrong-id", result={"echo": True})

    server = IPCServer(handler=mismatch_handler, transport=transport)
    await server.start()

    ep = _endpoint(sock, server.token)
    client = IPCClient(ep, transport=UnixSocketTransport(path=sock))
    await client.connect()

    try:
        with pytest.raises(ConnectionError, match="request_id mismatch"):
            await client.request(session_id="s1", capability="tasks", method="list")
        assert not client.is_connected
    finally:
        await client.close()
        await server.stop()


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
async def test_client_handles_large_response_line(short_tmp) -> None:
    """Client reads large single-line JSON responses without framing errors."""
    sock = str(short_tmp / "t.sock")
    transport = UnixSocketTransport(path=sock)
    large_payload = "x" * (128 * 1024)

    async def large_handler(req: CoreRequest) -> CoreResponse:
        return CoreResponse.success(req.request_id, result={"payload": large_payload})

    server = IPCServer(handler=large_handler, transport=transport)
    await server.start()

    ep = _endpoint(sock, server.token)
    client = IPCClient(ep, transport=UnixSocketTransport(path=sock))
    await client.connect()

    try:
        resp = await client.request(session_id="s1", capability="tasks", method="list")
        assert resp.ok
        assert resp.result is not None
        assert resp.result["payload"] == large_payload
    finally:
        await client.close()
        await server.stop()
