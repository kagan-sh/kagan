"""Unit tests for instance lock edge-case behavior and lock info model."""

from __future__ import annotations

import os
from typing import TYPE_CHECKING

import pytest
from filelock import Timeout

from kagan.core.instance_lock import InstanceLock, LockInfo

if TYPE_CHECKING:
    from pathlib import Path


class TestGracefulBehavior:
    """Fast deterministic tests for non-process lock behavior."""

    @staticmethod
    def _make_lock(tmp_path: Path, repo_name: str = "repo") -> InstanceLock:
        repo = tmp_path / repo_name
        repo.mkdir(exist_ok=True)
        return InstanceLock(repo, locks_dir=tmp_path / "locks")

    def test_multiple_acquire_calls_are_safe(self, tmp_path: Path) -> None:
        lock = self._make_lock(tmp_path)

        assert lock.acquire() is True
        assert lock.acquire() is True
        assert lock.is_held is True

        lock.release()

    @pytest.mark.parametrize("acquired_first", [True, False])
    def test_release_is_safe(self, tmp_path: Path, acquired_first: bool) -> None:
        lock = self._make_lock(tmp_path)
        if acquired_first:
            assert lock.acquire() is True
        lock.release()

        assert lock.is_held is False
        assert lock.get_holder_info() is None

    def test_context_manager_releases_on_exit(self, tmp_path: Path) -> None:
        lock = self._make_lock(tmp_path)

        with lock:
            assert lock.is_held

        assert not lock.is_held

        other_lock = self._make_lock(tmp_path)
        assert other_lock.acquire() is True
        other_lock.release()

    def test_context_manager_releases_on_exception(self, tmp_path: Path) -> None:
        lock = self._make_lock(tmp_path)

        with pytest.raises(ValueError):
            with lock:
                assert lock.is_held
                raise ValueError("Simulated error")

        assert not lock.is_held

        other_lock = self._make_lock(tmp_path)
        assert other_lock.acquire() is True
        other_lock.release()

    def test_context_manager_raises_timeout_when_locked(self, tmp_path: Path) -> None:
        first_lock = self._make_lock(tmp_path)
        first_lock.acquire()

        try:
            second_lock = self._make_lock(tmp_path)
            with pytest.raises(Timeout):
                with second_lock:
                    pass
        finally:
            first_lock.release()

    def test_holder_info_handles_corrupted_file(self, tmp_path: Path) -> None:
        lock = self._make_lock(tmp_path)
        lock.acquire()

        info_files = list((tmp_path / "locks").glob("*.info"))
        assert len(info_files) == 1
        info_files[0].write_text("not_a_number\n")

        other = self._make_lock(tmp_path)
        assert other.get_holder_info() is None
        lock.release()

    def test_lock_on_nonexistent_path(self, tmp_path: Path) -> None:
        lock = self._make_lock(tmp_path, repo_name="future_repo")

        assert lock.acquire() is True
        lock.release()


class TestLockInfo:
    """LockInfo dataclass behavior."""

    def test_lock_info_is_immutable(self) -> None:
        info = LockInfo(pid=123, hostname="test-host")

        with pytest.raises(AttributeError):
            info.pid = 456  # type: ignore[misc]

    def test_lock_info_contains_repo_path(self, tmp_path: Path) -> None:
        repo = tmp_path / "repo"
        repo.mkdir()

        lock = InstanceLock(repo, locks_dir=tmp_path / "locks")
        lock.acquire()

        other = InstanceLock(repo, locks_dir=tmp_path / "locks")
        info = other.get_holder_info()

        assert info is not None
        assert isinstance(info.pid, int)
        assert isinstance(info.hostname, str)
        assert info.pid == os.getpid()
        assert info.repo_path == str(repo.resolve())

        lock.release()
