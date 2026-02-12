"""Smoke tests for TUI attachment to a discovered core endpoint."""

from __future__ import annotations

import shutil
import sys
import tempfile
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from kagan.core.ipc.contracts import CoreRequest, CoreResponse
from kagan.core.ipc.discovery import CoreEndpoint
from kagan.core.ipc.server import IPCServer
from kagan.core.ipc.transports import UnixSocketTransport

if TYPE_CHECKING:
    from collections.abc import Generator

pytestmark = pytest.mark.usefixtures("global_mock_tmux")


@pytest.fixture
def _short_tmp_dir() -> Generator[Path, None, None]:
    """Create a short temp directory for Unix socket paths."""
    path = Path(tempfile.mkdtemp(prefix="k-tui-", dir="/tmp"))
    try:
        yield path
    finally:
        shutil.rmtree(path, ignore_errors=True)


@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
@pytest.mark.asyncio
async def test_tui_attaches_to_discovered_core_endpoint(
    e2e_project, monkeypatch: pytest.MonkeyPatch, _short_tmp_dir: Path
) -> None:
    """KaganApp should attach to a reachable discovered core endpoint."""
    from kagan.tui.app import KaganApp

    socket_path = _short_tmp_dir / "core.sock"

    async def _handler(request: CoreRequest) -> CoreResponse:
        return CoreResponse.success(request.request_id, result={"ok": True})

    server = IPCServer(
        handler=_handler,
        transport=UnixSocketTransport(path=str(socket_path)),
    )
    await server.start()

    endpoint = CoreEndpoint(
        transport="socket",
        address=str(socket_path),
        token=server.token,
    )
    monkeypatch.setattr("kagan.core.ipc.discovery.discover_core_endpoint", lambda: endpoint)

    app = KaganApp(
        db_path=e2e_project.db,
        config_path=e2e_project.config,
        project_root=e2e_project.root,
    )

    try:
        async with app.run_test(size=(120, 40)) as pilot:
            for _ in range(80):
                if app._core_status == "CONNECTED" and app._core_client is not None:
                    break
                await pilot.pause(0.1)
            else:
                raise AssertionError("TUI did not attach to discovered core endpoint")
    finally:
        await server.stop()
