"""Core runtime lease lock with heartbeat and stale reclaim."""

from __future__ import annotations

import contextlib
import json
import logging
import os
import socket
from dataclasses import dataclass
from datetime import datetime
from typing import TYPE_CHECKING

from filelock import FileLock, Timeout

from kagan.core.process_liveness import pid_exists
from kagan.core.time import utc_now

if TYPE_CHECKING:
    from pathlib import Path

logger = logging.getLogger(__name__)

CORE_LEASE_VERSION = 1
CORE_LEASE_HEARTBEAT_SECONDS = 2.0
CORE_LEASE_STALE_SECONDS = 10.0


@dataclass(frozen=True, slots=True)
class CoreLeaseRecord:
    version: int
    owner_pid: int
    owner_hostname: str
    acquired_at: str
    last_heartbeat_at: str
    heartbeat_interval_seconds: float
    stale_after_seconds: float
    stale_reclaim_rules: dict[str, bool | float]


def _parse_utc_timestamp(value: str) -> datetime | None:
    try:
        parsed = datetime.fromisoformat(value)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        return None
    return parsed


class CoreInstanceLock:
    """Cross-process singleton lock with explicit lease metadata."""

    def __init__(self, path: Path, *, lease_path: Path) -> None:
        self._path = path
        self._lease_path = lease_path
        self._lock = FileLock(str(path), blocking=False)
        self._acquired = False

    def acquire(self, *, _retry_stale: bool = True) -> bool:
        self._path.parent.mkdir(parents=True, exist_ok=True)
        if not self._try_acquire_lock():
            if _retry_stale and self._cleanup_stale_lease():
                self._lock = FileLock(str(self._path), blocking=False)
                return self.acquire(_retry_stale=False)
            return False

        self._acquired = True
        self._write_lease_record(heartbeat_at=utc_now())
        return True

    def _try_acquire_lock(self) -> bool:
        try:
            self._lock.acquire(timeout=0)
        except Timeout:
            return False
        return True

    @staticmethod
    def _pid_is_running(pid: int) -> bool:
        return pid_exists(pid)

    def _read_lease_record(self) -> CoreLeaseRecord | None:
        try:
            raw = self._lease_path.read_text(encoding="utf-8")
            data = json.loads(raw)
        except (OSError, json.JSONDecodeError, TypeError, ValueError):
            return None

        if not isinstance(data, dict):
            return None
        try:
            rules = data["stale_reclaim_rules"]
            if not isinstance(rules, dict):
                return None
            return CoreLeaseRecord(
                version=int(data["version"]),
                owner_pid=int(data["owner_pid"]),
                owner_hostname=str(data["owner_hostname"]),
                acquired_at=str(data["acquired_at"]),
                last_heartbeat_at=str(data["last_heartbeat_at"]),
                heartbeat_interval_seconds=float(data["heartbeat_interval_seconds"]),
                stale_after_seconds=float(data["stale_after_seconds"]),
                stale_reclaim_rules={
                    str(key): bool(value) if isinstance(value, bool) else float(value)
                    for key, value in rules.items()
                },
            )
        except (KeyError, TypeError, ValueError):
            return None

    def _is_stale_lease(self, lease: CoreLeaseRecord) -> bool:
        if lease.owner_pid == os.getpid():
            return False
        if lease.owner_hostname and lease.owner_hostname != socket.gethostname():
            return False
        heartbeat = _parse_utc_timestamp(lease.last_heartbeat_at)
        if heartbeat is None:
            return False
        heartbeat_age = (utc_now() - heartbeat).total_seconds()
        if heartbeat_age < max(lease.stale_after_seconds, 0.0):
            return False
        return not self._pid_is_running(lease.owner_pid)

    def _cleanup_stale_lease(self) -> bool:
        lease = self._read_lease_record()
        if lease is None or not self._is_stale_lease(lease):
            return False
        logger.warning(
            "Reclaiming stale core lease (pid=%s heartbeat=%s)",
            lease.owner_pid,
            lease.last_heartbeat_at,
        )
        with contextlib.suppress(OSError):
            self._lease_path.unlink(missing_ok=True)
        with contextlib.suppress(OSError):
            self._path.unlink(missing_ok=True)
        return True

    def _write_lease_record(self, *, heartbeat_at: datetime) -> None:
        lease = CoreLeaseRecord(
            version=CORE_LEASE_VERSION,
            owner_pid=os.getpid(),
            owner_hostname=socket.gethostname(),
            acquired_at=heartbeat_at.isoformat(),
            last_heartbeat_at=heartbeat_at.isoformat(),
            heartbeat_interval_seconds=CORE_LEASE_HEARTBEAT_SECONDS,
            stale_after_seconds=CORE_LEASE_STALE_SECONDS,
            stale_reclaim_rules={
                "same_host_required": True,
                "pid_must_be_dead": True,
                "heartbeat_age_must_exceed_seconds": CORE_LEASE_STALE_SECONDS,
            },
        )
        self._lease_path.write_text(
            json.dumps(
                {
                    "version": lease.version,
                    "owner_pid": lease.owner_pid,
                    "owner_hostname": lease.owner_hostname,
                    "acquired_at": lease.acquired_at,
                    "last_heartbeat_at": lease.last_heartbeat_at,
                    "heartbeat_interval_seconds": lease.heartbeat_interval_seconds,
                    "stale_after_seconds": lease.stale_after_seconds,
                    "stale_reclaim_rules": lease.stale_reclaim_rules,
                },
                sort_keys=True,
                indent=2,
            ),
            encoding="utf-8",
        )

    def heartbeat(self) -> None:
        if not self._acquired:
            return
        current = self._read_lease_record()
        if current is None:
            self._write_lease_record(heartbeat_at=utc_now())
            return
        heartbeat_at = utc_now().isoformat()
        payload = {
            "version": current.version,
            "owner_pid": current.owner_pid,
            "owner_hostname": current.owner_hostname,
            "acquired_at": current.acquired_at,
            "last_heartbeat_at": heartbeat_at,
            "heartbeat_interval_seconds": current.heartbeat_interval_seconds,
            "stale_after_seconds": current.stale_after_seconds,
            "stale_reclaim_rules": current.stale_reclaim_rules,
        }
        self._lease_path.write_text(
            json.dumps(payload, sort_keys=True, indent=2),
            encoding="utf-8",
        )

    def release(self) -> None:
        if not self._acquired:
            return

        try:
            self._lock.release()
        finally:
            self._acquired = False
        with contextlib.suppress(OSError):
            self._path.unlink()
        with contextlib.suppress(OSError):
            self._lease_path.unlink(missing_ok=True)
