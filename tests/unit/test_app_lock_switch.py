from __future__ import annotations

import os
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, cast
from unittest.mock import Mock

from kagan.app import KaganApp
from kagan.instance_lock import LockInfo

if TYPE_CHECKING:
    from pathlib import Path


async def test_try_switch_lock_treats_same_pid_holder_as_current_instance(
    monkeypatch,
    tmp_path: Path,
) -> None:
    app = KaganApp(db_path=":memory:", project_root=tmp_path / "repo-a")
    current_lock = SimpleNamespace(release=Mock())
    app._instance_lock = cast("Any", current_lock)

    class SamePidLock:
        def __init__(self, _repo_path: Path) -> None:
            pass

        def acquire(self) -> bool:
            return False

        def get_holder_info(self) -> LockInfo:
            return LockInfo(pid=os.getpid(), hostname="local", repo_path=str(tmp_path / "repo-b"))

    monkeypatch.setattr("kagan.app.InstanceLock", SamePidLock)

    result = await app._try_switch_lock(tmp_path / "repo-b")

    assert result is None
    current_lock.release.assert_not_called()
    assert app._instance_lock is current_lock


async def test_try_switch_lock_returns_holder_for_other_process(
    monkeypatch,
    tmp_path: Path,
) -> None:
    app = KaganApp(db_path=":memory:", project_root=tmp_path / "repo-a")
    current_lock = SimpleNamespace(release=Mock())
    app._instance_lock = cast("Any", current_lock)

    class OtherPidLock:
        def __init__(self, _repo_path: Path) -> None:
            pass

        def acquire(self) -> bool:
            return False

        def get_holder_info(self) -> LockInfo:
            return LockInfo(pid=os.getpid() + 1000, hostname="other", repo_path=None)

    monkeypatch.setattr("kagan.app.InstanceLock", OtherPidLock)

    result = await app._try_switch_lock(tmp_path / "repo-b")

    assert result is not None
    assert result.hostname == "other"
    current_lock.release.assert_not_called()
