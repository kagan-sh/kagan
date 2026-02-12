from __future__ import annotations

from time import monotonic
from types import SimpleNamespace
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock

from tests.helpers.wait import wait_until

from kagan.core.config import KaganConfig
from kagan.core.host import CoreHost, CoreHostStatus
from kagan.core.ipc.transports import ServerHandle

if TYPE_CHECKING:
    from collections.abc import Callable
    from typing import Any


class _FakeIPCServer:
    def __init__(
        self,
        *,
        handler: object,
        transport_preference: str,
        on_client_connect: Callable[[], None] | None = None,
        on_client_disconnect: Callable[[], None] | None = None,
    ) -> None:
        del handler
        self.transport_preference = transport_preference
        self.on_client_connect = on_client_connect
        self.on_client_disconnect = on_client_disconnect
        self.stop = AsyncMock()

    async def start(self) -> ServerHandle:
        return ServerHandle(
            transport_type="socket",
            address="/tmp/kagan-core.sock",
            port=None,
            close=None,
        )


async def test_core_host_starts_automation_service(monkeypatch) -> None:
    config = KaganConfig()
    config.general.core_idle_timeout_seconds = 0

    ctx = SimpleNamespace(
        automation_service=SimpleNamespace(start=AsyncMock(), stop=AsyncMock()),
        runtime_service=SimpleNamespace(
            reconcile_startup_state=AsyncMock(),
            reconcile_running_tasks=AsyncMock(),
        ),
        task_service=SimpleNamespace(list_tasks=AsyncMock(return_value=[])),
        event_bus=SimpleNamespace(publish=AsyncMock()),
        close=AsyncMock(),
    )

    async def _fake_create_app_context(*args: object, **kwargs: object) -> SimpleNamespace:
        del args, kwargs
        return ctx

    monkeypatch.setattr("kagan.core.host.create_app_context", _fake_create_app_context)
    monkeypatch.setattr("kagan.core.host.IPCServer", _FakeIPCServer)
    monkeypatch.setattr(CoreHost, "_write_runtime_files", lambda self, handle: None)
    monkeypatch.setattr(CoreHost, "_cleanup_runtime_files", lambda self: None)

    host = CoreHost(config=config)
    await host.start()

    assert host.status is CoreHostStatus.RUNNING
    ctx.automation_service.start.assert_awaited_once()

    await host.stop(reason="test teardown")
    assert host.status == CoreHostStatus.STOPPED


async def test_core_host_idle_timeout_waits_for_disconnect(monkeypatch) -> None:
    config = KaganConfig()
    config.general.core_idle_timeout_seconds = 1

    ctx = SimpleNamespace(
        automation_service=SimpleNamespace(start=AsyncMock(), stop=AsyncMock()),
        runtime_service=SimpleNamespace(
            reconcile_startup_state=AsyncMock(),
            reconcile_running_tasks=AsyncMock(),
        ),
        task_service=SimpleNamespace(list_tasks=AsyncMock(return_value=[])),
        event_bus=SimpleNamespace(publish=AsyncMock()),
        close=AsyncMock(),
    )

    fake_server: _FakeIPCServer | None = None

    def _fake_server_factory(**kwargs: Any) -> _FakeIPCServer:
        nonlocal fake_server
        fake_server = _FakeIPCServer(**kwargs)
        return fake_server

    async def _fake_create_app_context(*args: object, **kwargs: object) -> SimpleNamespace:
        del args, kwargs
        return ctx

    monkeypatch.setattr("kagan.core.host.create_app_context", _fake_create_app_context)
    monkeypatch.setattr("kagan.core.host.IPCServer", _fake_server_factory)
    monkeypatch.setattr(CoreHost, "_write_runtime_files", lambda self, handle: None)
    monkeypatch.setattr(CoreHost, "_cleanup_runtime_files", lambda self: None)

    host = CoreHost(config=config)
    await host.start()
    assert host.status is CoreHostStatus.RUNNING
    assert fake_server is not None
    assert callable(fake_server.on_client_connect)
    assert callable(fake_server.on_client_disconnect)

    fake_server.on_client_connect()
    start = monotonic()
    await wait_until(
        lambda: monotonic() - start >= 1.2,
        timeout=1.5,
        check_interval=0.05,
        description="idle timeout window while connected",
    )
    assert host.status is CoreHostStatus.RUNNING

    fake_server.on_client_disconnect()
    await wait_until(
        lambda: host.status == CoreHostStatus.STOPPED,
        timeout=1.5,
        check_interval=0.05,
        description="core host stop after disconnect",
    )
    assert host.status == CoreHostStatus.STOPPED
