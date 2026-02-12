from __future__ import annotations

import json
import os
import socket
from datetime import timedelta
from types import SimpleNamespace
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock

from kagan.core.config import KaganConfig
from kagan.core.host import CoreHost
from kagan.core.instance_lease import CoreInstanceLock
from kagan.core.ipc.transports import ServerHandle
from kagan.core.models.enums import TaskType
from kagan.core.time import utc_now

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path


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
        self.token = "test-token"
        self.stop = AsyncMock()

    async def start(self) -> ServerHandle:
        return ServerHandle(
            transport_type="socket",
            address="/tmp/kagan-core.sock",
            port=None,
            close=None,
        )


def _make_context(*, tasks: list[SimpleNamespace] | None = None) -> SimpleNamespace:
    runtime_service = SimpleNamespace(
        reconcile_startup_state=AsyncMock(),
        reconcile_running_tasks=AsyncMock(),
    )
    return SimpleNamespace(
        automation_service=SimpleNamespace(start=AsyncMock(), stop=AsyncMock()),
        runtime_service=runtime_service,
        task_service=SimpleNamespace(list_tasks=AsyncMock(return_value=tasks or [])),
        event_bus=SimpleNamespace(publish=AsyncMock()),
        close=AsyncMock(),
    )


def _stale_lease_payload(pid: int) -> dict[str, object]:
    stale_time = (utc_now() - timedelta(seconds=120)).isoformat()
    return {
        "version": 1,
        "owner_pid": pid,
        "owner_hostname": socket.gethostname(),
        "acquired_at": stale_time,
        "last_heartbeat_at": stale_time,
        "heartbeat_interval_seconds": 2.0,
        "stale_after_seconds": 10.0,
        "stale_reclaim_rules": {
            "same_host_required": True,
            "pid_must_be_dead": True,
            "heartbeat_age_must_exceed_seconds": 10.0,
        },
    }


async def test_core_host_runtime_files_use_lease_not_pid(monkeypatch, tmp_path: Path) -> None:
    config = KaganConfig()
    config.general.core_idle_timeout_seconds = 0
    runtime_dir = tmp_path / "core-runtime"
    runtime_dir.mkdir(parents=True, exist_ok=True)
    ctx = _make_context()

    async def _fake_create_app_context(*args: object, **kwargs: object) -> SimpleNamespace:
        del args, kwargs
        return ctx

    monkeypatch.setattr("kagan.core.host.create_app_context", _fake_create_app_context)
    monkeypatch.setattr("kagan.core.host.IPCServer", _FakeIPCServer)
    monkeypatch.setenv("KAGAN_CORE_RUNTIME_DIR", str(runtime_dir))

    host = CoreHost(config=config)
    await host.start()

    lease_path = runtime_dir / "core.lease.json"
    endpoint_path = runtime_dir / "endpoint.json"
    token_path = runtime_dir / "token"
    pid_path = runtime_dir / "core.pid"

    assert lease_path.exists()
    assert endpoint_path.exists()
    assert token_path.exists()
    assert not pid_path.exists()
    lease = json.loads(lease_path.read_text(encoding="utf-8"))
    assert lease["owner_pid"] == os.getpid()
    assert "acquired_at" in lease
    assert "last_heartbeat_at" in lease
    assert lease["stale_reclaim_rules"]["pid_must_be_dead"] is True
    assert token_path.read_text(encoding="utf-8") == "test-token"

    await host.stop(reason="test teardown")

    assert not lease_path.exists()
    assert not endpoint_path.exists()
    assert not token_path.exists()


async def test_core_host_startup_reconciles_runtime_state(monkeypatch, tmp_path: Path) -> None:
    config = KaganConfig()
    config.general.core_idle_timeout_seconds = 0
    runtime_dir = tmp_path / "core-runtime"
    runtime_dir.mkdir(parents=True, exist_ok=True)
    tasks = [
        SimpleNamespace(id="auto-1", task_type=TaskType.AUTO),
        SimpleNamespace(id="pair-1", task_type=TaskType.PAIR),
        SimpleNamespace(id="auto-2", task_type=TaskType.AUTO),
    ]
    ctx = _make_context(tasks=tasks)

    async def _fake_create_app_context(*args: object, **kwargs: object) -> SimpleNamespace:
        del args, kwargs
        return ctx

    monkeypatch.setattr("kagan.core.host.create_app_context", _fake_create_app_context)
    monkeypatch.setattr("kagan.core.host.IPCServer", _FakeIPCServer)
    monkeypatch.setenv("KAGAN_CORE_RUNTIME_DIR", str(runtime_dir))

    host = CoreHost(config=config)
    await host.start()

    ctx.runtime_service.reconcile_startup_state.assert_awaited_once()
    ctx.runtime_service.reconcile_running_tasks.assert_awaited_once_with(["auto-1", "auto-2"])

    await host.stop(reason="test teardown")


def test_core_instance_lock_retries_once_for_stale_lease(monkeypatch, tmp_path: Path) -> None:
    lock_path = tmp_path / "core.instance.lock"
    lease_path = tmp_path / "core.lease.json"
    lease_path.write_text(
        json.dumps(_stale_lease_payload(424242), indent=2),
        encoding="utf-8",
    )
    lock = CoreInstanceLock(lock_path, lease_path=lease_path)
    attempts = {"count": 0}
    original_try = CoreInstanceLock._try_acquire_lock

    def _fake_try(self):
        attempts["count"] += 1
        if attempts["count"] == 1:
            return False
        return original_try(self)

    monkeypatch.setattr(CoreInstanceLock, "_try_acquire_lock", _fake_try)
    monkeypatch.setattr(CoreInstanceLock, "_pid_is_running", staticmethod(lambda _pid: False))

    assert lock.acquire() is True
    assert attempts["count"] == 2
    lease = json.loads(lease_path.read_text(encoding="utf-8"))
    assert lease["owner_pid"] == os.getpid()
    lock.release()


def test_core_instance_lock_does_not_reclaim_live_lease(monkeypatch, tmp_path: Path) -> None:
    lock_path = tmp_path / "core.instance.lock"
    lease_path = tmp_path / "core.lease.json"
    lease_path.write_text(
        json.dumps(_stale_lease_payload(424242), indent=2),
        encoding="utf-8",
    )
    lock = CoreInstanceLock(lock_path, lease_path=lease_path)
    attempts = {"count": 0}

    def _always_fail(_self) -> bool:
        attempts["count"] += 1
        return False

    monkeypatch.setattr(CoreInstanceLock, "_try_acquire_lock", _always_fail)
    monkeypatch.setattr(CoreInstanceLock, "_pid_is_running", staticmethod(lambda _pid: True))

    assert lock.acquire() is False
    assert attempts["count"] == 1
