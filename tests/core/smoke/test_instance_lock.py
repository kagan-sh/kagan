"""Smoke tests for per-repository instance locking across processes and path forms."""

from __future__ import annotations

import multiprocessing
import os
import platform
from pathlib import Path

import pytest

from kagan.core.instance_lock import InstanceLock

_IS_WINDOWS = platform.system() == "Windows"


class TestSingleInstanceEnforcement:
    """Only one process should hold a repository lock at a time."""

    def test_first_instance_starts_successfully(self, tmp_path: Path) -> None:
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)
        started = lock.acquire()

        assert started is True
        lock.release()

    def test_second_instance_blocked_shows_who_holds_lock(self, tmp_path: Path) -> None:
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        first_instance = InstanceLock(repo, locks_dir=locks_dir)
        second_instance = InstanceLock(repo, locks_dir=locks_dir)

        first_instance.acquire()
        could_start = second_instance.acquire()

        assert could_start is False

        holder_info = second_instance.get_holder_info()
        assert holder_info is not None
        assert holder_info.pid == os.getpid()
        assert len(holder_info.hostname) > 0
        assert holder_info.repo_path == str(repo.resolve())

        first_instance.release()

    def test_instance_becomes_available_after_first_closes(self, tmp_path: Path) -> None:
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        first_instance = InstanceLock(repo, locks_dir=locks_dir)
        second_instance = InstanceLock(repo, locks_dir=locks_dir)

        first_instance.acquire()
        first_instance.release()

        started = second_instance.acquire()

        assert started is True
        second_instance.release()


class TestLockFileLocation:
    """Lock files should be isolated from user repositories."""

    def test_lock_files_not_in_repo(self, tmp_path: Path) -> None:
        repo = tmp_path / "my_project"
        repo.mkdir()
        locks_dir = tmp_path / "xdg_state" / "kagan" / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)
        lock.acquire()

        repo_files = list(repo.iterdir())
        assert not any(f.name.startswith(".kagan") for f in repo_files)

        lock_files = list(locks_dir.iterdir())
        assert len(lock_files) == 2
        assert any(f.suffix == ".lock" for f in lock_files)
        assert any(f.suffix == ".info" for f in lock_files)

        lock.release()

    def test_lock_file_cleaned_up_on_release(self, tmp_path: Path) -> None:
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)
        lock.acquire()

        assert len(list(locks_dir.iterdir())) == 2

        lock.release()

        assert len(list(locks_dir.iterdir())) == 0

    def test_different_repos_get_different_locks(self, tmp_path: Path) -> None:
        repo_a = tmp_path / "repo_a"
        repo_b = tmp_path / "repo_b"
        repo_a.mkdir()
        repo_b.mkdir()
        locks_dir = tmp_path / "locks"

        lock_a = InstanceLock(repo_a, locks_dir=locks_dir)
        lock_b = InstanceLock(repo_b, locks_dir=locks_dir)

        assert lock_a.acquire() is True
        assert lock_b.acquire() is True

        assert len(list(locks_dir.iterdir())) == 4

        lock_a.release()
        lock_b.release()


def _subprocess_try_acquire(
    repo_path: str, locks_dir: str, result_queue: multiprocessing.Queue
) -> None:
    """Try to acquire lock in a subprocess and report result via queue."""
    lock = InstanceLock(Path(repo_path), locks_dir=Path(locks_dir))
    acquired = lock.acquire()
    result_queue.put(acquired)
    if acquired:
        lock.release()


@pytest.mark.skipif(
    _IS_WINDOWS,
    reason="multiprocessing.Process uses 'spawn' on Windows which re-imports conftest "
    "top-level code and hangs under pytest-xdist",
)
class TestCrossProcessLocking:
    """Real subprocess behavior for lock exclusivity."""

    def test_second_process_cannot_acquire_lock(self, tmp_path: Path) -> None:
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)
        lock.acquire()

        result_queue: multiprocessing.Queue = multiprocessing.Queue()
        proc = multiprocessing.Process(
            target=_subprocess_try_acquire,
            args=(str(repo), str(locks_dir), result_queue),
        )
        proc.start()
        proc.join(timeout=5)

        subprocess_acquired = result_queue.get(timeout=2)
        assert subprocess_acquired is False

        lock.release()

    def test_lock_released_when_process_exits(self, tmp_path: Path) -> None:
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        result_queue: multiprocessing.Queue = multiprocessing.Queue()
        proc = multiprocessing.Process(
            target=_subprocess_try_acquire,
            args=(str(repo), str(locks_dir), result_queue),
        )
        proc.start()
        proc.join(timeout=5)

        first_result = result_queue.get(timeout=2)
        assert first_result is True

        lock = InstanceLock(repo, locks_dir=locks_dir)
        acquired = lock.acquire()
        assert acquired is True
        lock.release()


class TestPathCanonicalization:
    """Canonical path handling should map equivalent paths to one lock."""

    @pytest.mark.skipif(
        _IS_WINDOWS,
        reason="symlinks require SeCreateSymbolicLinkPrivilege on Windows",
    )
    def test_symlink_resolves_to_same_lock(self, tmp_path: Path) -> None:
        repo = tmp_path / "actual_repo"
        repo.mkdir()
        symlink = tmp_path / "symlink_repo"
        symlink.symlink_to(repo)
        locks_dir = tmp_path / "locks"

        lock_via_real = InstanceLock(repo, locks_dir=locks_dir)
        lock_via_symlink = InstanceLock(symlink, locks_dir=locks_dir)

        assert lock_via_real.acquire() is True
        assert lock_via_symlink.acquire() is False

        lock_via_real.release()

    @pytest.mark.skipif(
        _IS_WINDOWS,
        reason="os.chdir() is process-global and unsafe under pytest-xdist workers on Windows",
    )
    def test_relative_and_absolute_paths_same_lock(self, tmp_path: Path) -> None:
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        original_cwd = Path.cwd()
        try:
            os.chdir(tmp_path)

            lock_absolute = InstanceLock(repo, locks_dir=locks_dir)
            lock_relative = InstanceLock(Path("repo"), locks_dir=locks_dir)

            assert lock_absolute.acquire() is True
            assert lock_relative.acquire() is False

            lock_absolute.release()
        finally:
            os.chdir(original_cwd)
