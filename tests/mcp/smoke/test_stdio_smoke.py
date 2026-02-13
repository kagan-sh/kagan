from __future__ import annotations

import asyncio
import json
import shutil
import sys
import tempfile
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from mcp.client.stdio import StdioServerParameters, stdio_client

from kagan.core.ipc.contracts import CoreRequest, CoreResponse
from kagan.core.ipc.server import IPCServer
from kagan.core.ipc.transports import UnixSocketTransport
from kagan.core.paths import get_core_endpoint_path, get_core_runtime_dir, get_core_token_path
from mcp import ClientSession

if TYPE_CHECKING:
    from collections.abc import Generator


@pytest.fixture
def _short_tmp_dir() -> Generator[Path, None, None]:
    """Create a short temp directory for Unix socket paths."""
    path = Path(tempfile.mkdtemp(prefix="k-mcp-", dir="/tmp"))
    try:
        yield path
    finally:
        shutil.rmtree(path, ignore_errors=True)


def _server_params(session_id: str = "mcp-smoke-session") -> StdioServerParameters:
    return StdioServerParameters(
        command="uv",
        args=[
            "run",
            "python",
            "-m",
            "kagan",
            "mcp",
            "--readonly",
            "--session-id",
            session_id,
        ],
        cwd=str(Path.cwd()),
    )


@pytest.mark.asyncio
async def test_mcp_stdio_smoke_lists_tools() -> None:
    async with stdio_client(_server_params()) as (read_stream, write_stream):
        async with ClientSession(read_stream, write_stream) as session:
            await session.initialize()
            tools = await session.list_tools()

    names = {tool.name for tool in tools.tools}
    assert "plan_submit" in names
    assert "task_get" in names
    assert "task_list" in names


@pytest.mark.asyncio
async def test_mcp_stdio_smoke_plan_submit() -> None:
    async with stdio_client(_server_params()) as (read_stream, write_stream):
        async with ClientSession(read_stream, write_stream) as session:
            await session.initialize()
            result = await session.call_tool(
                "plan_submit",
                {
                    "tasks": [
                        {
                            "title": "Smoke plan task",
                            "type": "AUTO",
                            "description": "Verify stdio tool call flow.",
                            "acceptance_criteria": ["Tool returns a structured response"],
                            "priority": "low",
                        }
                    ],
                    "todos": [
                        {"content": "Create smoke proposal payload", "status": "completed"},
                    ],
                },
            )

    assert result.isError is False
    rendered = "".join(getattr(item, "text", "") for item in result.content)
    assert "received" in rendered


async def _call_tasks_list(session_id: str) -> str:
    async with stdio_client(_server_params(session_id)) as (read_stream, write_stream):
        async with ClientSession(read_stream, write_stream) as session:
            await session.initialize()
            result = await session.call_tool("task_list", {})

    assert result.isError is False
    return "".join(getattr(item, "text", "") for item in result.content)


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
@pytest.mark.asyncio
async def test_mcp_stdio_smoke_two_clients_share_core(
    monkeypatch: pytest.MonkeyPatch, _short_tmp_dir: Path
) -> None:
    """Two MCP stdio clients can connect concurrently and call a core-backed tool."""
    runtime_dir = _short_tmp_dir / "runtime"
    runtime_dir.mkdir(parents=True, exist_ok=True)
    monkeypatch.setenv("KAGAN_CORE_RUNTIME_DIR", str(runtime_dir))

    socket_path = _short_tmp_dir / "core.sock"

    async def _handler(request: CoreRequest) -> CoreResponse:
        if request.capability == "tasks" and request.method == "list":
            return CoreResponse.success(request.request_id, result={"tasks": [], "count": 0})
        return CoreResponse.success(request.request_id, result={})

    server = IPCServer(
        handler=_handler,
        transport=UnixSocketTransport(path=str(socket_path)),
    )
    await server.start()

    endpoint_path = get_core_endpoint_path()
    token_path = get_core_token_path()
    get_core_runtime_dir().mkdir(parents=True, exist_ok=True)
    endpoint_path.write_text(
        json.dumps({"transport": "socket", "address": str(socket_path)}, indent=2),
        encoding="utf-8",
    )
    token_path.write_text(server.token, encoding="utf-8")

    try:
        rendered_a, rendered_b = await asyncio.gather(
            _call_tasks_list("mcp-smoke-session-a"),
            _call_tasks_list("mcp-smoke-session-b"),
        )
    finally:
        await server.stop()

    assert "count" in rendered_a
    assert "count" in rendered_b
