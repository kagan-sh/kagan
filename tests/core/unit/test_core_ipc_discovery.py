from __future__ import annotations

import json
import os
import socket
from typing import TYPE_CHECKING

from kagan.core.ipc.discovery import discover_core_endpoint

if TYPE_CHECKING:
    from pathlib import Path


def _write_runtime_files(
    runtime_dir: Path,
    *,
    owner_pid: int,
    transport: str = "socket",
    address: str = "/tmp/kagan-core.sock",
    port: int | None = None,
) -> None:
    runtime_dir.mkdir(parents=True, exist_ok=True)
    endpoint_payload: dict[str, object] = {"transport": transport, "address": address}
    if port is not None:
        endpoint_payload["port"] = port
    (runtime_dir / "endpoint.json").write_text(json.dumps(endpoint_payload), encoding="utf-8")
    (runtime_dir / "token").write_text("lease-token", encoding="utf-8")
    (runtime_dir / "core.lease.json").write_text(
        json.dumps(
            {
                "version": 1,
                "owner_pid": owner_pid,
                "owner_hostname": socket.gethostname(),
                "acquired_at": "2026-02-11T00:00:00+00:00",
                "last_heartbeat_at": "2026-02-11T00:00:01+00:00",
                "heartbeat_interval_seconds": 2.0,
                "stale_after_seconds": 10.0,
                "stale_reclaim_rules": {
                    "same_host_required": True,
                    "pid_must_be_dead": True,
                    "heartbeat_age_must_exceed_seconds": 10.0,
                },
            }
        ),
        encoding="utf-8",
    )


def test_discover_core_endpoint_reads_pid_from_lease(monkeypatch, tmp_path: Path) -> None:
    runtime_dir = tmp_path / "core-runtime"
    _write_runtime_files(runtime_dir, owner_pid=os.getpid())
    monkeypatch.setenv("KAGAN_CORE_RUNTIME_DIR", str(runtime_dir))

    endpoint = discover_core_endpoint()

    assert endpoint is not None
    assert endpoint.transport == "socket"
    assert endpoint.address == "/tmp/kagan-core.sock"
    assert endpoint.token == "lease-token"
    assert endpoint.pid == os.getpid()


def test_discover_core_endpoint_rejects_stale_dead_lease(monkeypatch, tmp_path: Path) -> None:
    runtime_dir = tmp_path / "core-runtime"
    _write_runtime_files(runtime_dir, owner_pid=424242)
    monkeypatch.setenv("KAGAN_CORE_RUNTIME_DIR", str(runtime_dir))
    monkeypatch.setattr("kagan.core.ipc.discovery._is_process_alive", lambda _pid: False)

    endpoint = discover_core_endpoint()

    assert endpoint is None


def test_discover_core_endpoint_rejects_unreachable_tcp_endpoint(
    monkeypatch, tmp_path: Path
) -> None:
    runtime_dir = tmp_path / "core-runtime"
    _write_runtime_files(
        runtime_dir,
        owner_pid=os.getpid(),
        transport="tcp",
        address="127.0.0.1",
        port=54321,
    )
    monkeypatch.setenv("KAGAN_CORE_RUNTIME_DIR", str(runtime_dir))
    monkeypatch.setattr("kagan.core.ipc.discovery._is_tcp_endpoint_reachable", lambda *_: False)

    endpoint = discover_core_endpoint()

    assert endpoint is None
