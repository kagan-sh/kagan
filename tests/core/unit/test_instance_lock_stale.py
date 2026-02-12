from __future__ import annotations

import socket
from typing import TYPE_CHECKING

from filelock import Timeout

from kagan.core.instance_lock import InstanceLock

if TYPE_CHECKING:
    from pathlib import Path


def _write_stale_info_file(repo: Path, locks_dir: Path, *, pid: int = 424242) -> None:
    """Create a stale lock-info file without touching private lock attributes."""
    seeder = InstanceLock(repo, locks_dir=locks_dir)
    assert seeder.acquire() is True
    info_files = list(locks_dir.glob("*.info"))
    assert len(info_files) == 1
    info_path = info_files[0]
    seeder.release()
    info_path.write_text(f"{pid}\n{socket.gethostname()}\n{repo}\n")


def test_acquire_retries_once_for_stale_local_holder(
    monkeypatch,
    tmp_path: Path,
) -> None:
    repo = tmp_path / "repo"
    repo.mkdir()
    locks_dir = tmp_path / "locks"
    lock = InstanceLock(repo, locks_dir=locks_dir)
    _write_stale_info_file(repo, locks_dir)

    attempts = {"count": 0}

    def _fake_acquire(self, timeout: float = 0) -> None:
        del self, timeout
        attempts["count"] += 1
        if attempts["count"] == 1:
            raise Timeout("locked")

    monkeypatch.setattr("filelock.FileLock.acquire", _fake_acquire)
    monkeypatch.setattr("filelock.FileLock.release", lambda self, *args, **kwargs: None)
    monkeypatch.setattr(InstanceLock, "_pid_is_running", staticmethod(lambda _pid: False))

    assert lock.acquire() is True
    assert attempts["count"] == 2
    assert lock.is_held is True
    lock.release()


def test_acquire_does_not_override_live_holder(
    monkeypatch,
    tmp_path: Path,
) -> None:
    repo = tmp_path / "repo"
    repo.mkdir()
    locks_dir = tmp_path / "locks"
    lock = InstanceLock(repo, locks_dir=locks_dir)
    _write_stale_info_file(repo, locks_dir)

    attempts = {"count": 0}

    def _always_timeout(self, timeout: float = 0) -> None:
        del self, timeout
        attempts["count"] += 1
        raise Timeout("locked")

    monkeypatch.setattr("filelock.FileLock.acquire", _always_timeout)
    monkeypatch.setattr(InstanceLock, "_pid_is_running", staticmethod(lambda _pid: True))

    assert lock.acquire() is False
    assert attempts["count"] == 1
