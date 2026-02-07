"""Tests for per-repository instance locking.

These tests verify the user-facing behavior: only one Kagan instance
can run per repository at a time. Lock files are stored in XDG state
directory to avoid polluting user repositories.
"""

from __future__ import annotations

import multiprocessing
import os
import time
from pathlib import Path

import pytest
from filelock import Timeout

from kagan.instance_lock import InstanceLock, LockInfo


class TestSingleInstanceEnforcement:
    """Tests that verify only one instance can run per repository."""

    @pytest.mark.integration
    def test_first_instance_starts_successfully(self, tmp_path: Path) -> None:
        """User can start Kagan in a repository with no other instance running."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)

        # Simulates: user runs `kagan` in a fresh repo
        started = lock.acquire()

        assert started is True, "First instance should start without issues"
        lock.release()

    @pytest.mark.integration
    def test_second_instance_blocked_shows_who_holds_lock(self, tmp_path: Path) -> None:
        """When user tries to start second instance, they see who has the lock."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        first_instance = InstanceLock(repo, locks_dir=locks_dir)
        second_instance = InstanceLock(repo, locks_dir=locks_dir)

        # User A starts Kagan
        first_instance.acquire()

        # User B (or same user in another terminal) tries to start Kagan
        could_start = second_instance.acquire()

        assert could_start is False, "Second instance should be blocked"

        # The blocked user should see helpful info about who holds the lock
        holder_info = second_instance.get_holder_info()
        assert holder_info is not None
        assert holder_info.pid == os.getpid()
        assert len(holder_info.hostname) > 0
        assert holder_info.repo_path == str(repo.resolve())

        first_instance.release()

    @pytest.mark.integration
    def test_instance_becomes_available_after_first_closes(self, tmp_path: Path) -> None:
        """After first instance closes, another user can start Kagan."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        first_instance = InstanceLock(repo, locks_dir=locks_dir)
        second_instance = InstanceLock(repo, locks_dir=locks_dir)

        # User A starts and then closes Kagan
        first_instance.acquire()
        first_instance.release()

        # User B can now start Kagan
        started = second_instance.acquire()

        assert started is True, "Should be able to start after first instance closes"
        second_instance.release()


class TestLockFileLocation:
    """Tests that verify lock files are stored in XDG directory, not repo."""

    @pytest.mark.integration
    def test_lock_files_not_in_repo(self, tmp_path: Path) -> None:
        """Lock files should not pollute the user's repository."""
        repo = tmp_path / "my_project"
        repo.mkdir()
        locks_dir = tmp_path / "xdg_state" / "kagan" / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)
        lock.acquire()

        # No lock files in the repo
        repo_files = list(repo.iterdir())
        assert not any(f.name.startswith(".kagan") for f in repo_files)

        # Lock files should be in the XDG directory
        lock_files = list(locks_dir.iterdir())
        assert len(lock_files) == 2  # .lock and .info files
        assert any(f.suffix == ".lock" for f in lock_files)
        assert any(f.suffix == ".info" for f in lock_files)

        lock.release()

    @pytest.mark.integration
    def test_lock_file_cleaned_up_on_release(self, tmp_path: Path) -> None:
        """Lock files are removed when instance closes cleanly."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)
        lock.acquire()

        assert len(list(locks_dir.iterdir())) == 2  # .lock and .info

        lock.release()

        # Clean exit should remove both lock files
        assert len(list(locks_dir.iterdir())) == 0

    @pytest.mark.integration
    def test_different_repos_get_different_locks(self, tmp_path: Path) -> None:
        """Each repository gets its own unique lock file."""
        repo_a = tmp_path / "repo_a"
        repo_b = tmp_path / "repo_b"
        repo_a.mkdir()
        repo_b.mkdir()
        locks_dir = tmp_path / "locks"

        lock_a = InstanceLock(repo_a, locks_dir=locks_dir)
        lock_b = InstanceLock(repo_b, locks_dir=locks_dir)

        # Both repos can be locked simultaneously
        assert lock_a.acquire() is True
        assert lock_b.acquire() is True

        # 4 files total: 2 per repo (.lock + .info)
        assert len(list(locks_dir.iterdir())) == 4

        lock_a.release()
        lock_b.release()


class TestGracefulBehavior:
    """Tests for edge cases and graceful handling."""

    @pytest.mark.unit
    def test_multiple_acquire_calls_are_safe(self, tmp_path: Path) -> None:
        """Calling acquire multiple times doesn't cause issues."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)

        # Some code paths might call acquire() more than once
        assert lock.acquire() is True
        assert lock.acquire() is True  # Should still work
        assert lock.is_held is True

        lock.release()

    @pytest.mark.unit
    def test_release_without_acquire_is_safe(self, tmp_path: Path) -> None:
        """Calling release without acquire doesn't crash."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)

        # This shouldn't raise - handles edge case of cleanup code
        lock.release()

    @pytest.mark.unit
    def test_holder_info_unavailable_when_no_lock(self, tmp_path: Path) -> None:
        """get_holder_info returns None gracefully when no lock exists."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)

        # No lock file exists yet
        info = lock.get_holder_info()

        assert info is None

    @pytest.mark.unit
    def test_context_manager_releases_on_exit(self, tmp_path: Path) -> None:
        """Lock is automatically released when using context manager."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)

        with lock:
            assert lock.is_held

        # Should be released after exiting context
        assert not lock.is_held

        # Another instance should now be able to acquire
        other_lock = InstanceLock(repo, locks_dir=locks_dir)
        assert other_lock.acquire() is True
        other_lock.release()

    @pytest.mark.unit
    def test_context_manager_releases_on_exception(self, tmp_path: Path) -> None:
        """Lock is released even when exception occurs inside context."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)

        with pytest.raises(ValueError):
            with lock:
                assert lock.is_held
                raise ValueError("Simulated error")

        # Lock should still be released after exception
        assert not lock.is_held

        # Another instance should be able to acquire
        other_lock = InstanceLock(repo, locks_dir=locks_dir)
        assert other_lock.acquire() is True
        other_lock.release()

    @pytest.mark.unit
    def test_context_manager_raises_timeout_when_locked(self, tmp_path: Path) -> None:
        """Context manager raises Timeout when lock cannot be acquired."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        first_lock = InstanceLock(repo, locks_dir=locks_dir)
        first_lock.acquire()

        try:
            second_lock = InstanceLock(repo, locks_dir=locks_dir)

            with pytest.raises(Timeout):
                with second_lock:
                    pass  # Should never reach here
        finally:
            first_lock.release()

    @pytest.mark.unit
    def test_holder_info_handles_corrupted_file(self, tmp_path: Path) -> None:
        """get_holder_info returns None gracefully for corrupted info files."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)
        lock.acquire()

        # Corrupt the info file
        info_files = list(locks_dir.glob("*.info"))
        assert len(info_files) == 1
        info_files[0].write_text("not_a_number\n")

        other = InstanceLock(repo, locks_dir=locks_dir)
        info = other.get_holder_info()

        # Should handle gracefully, not crash
        assert info is None

        lock.release()

    @pytest.mark.unit
    def test_lock_on_nonexistent_path(self, tmp_path: Path) -> None:
        """Lock can be acquired for a path that doesn't exist yet."""
        repo = tmp_path / "future_repo"  # Does not exist
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)

        # Should work - repo might be created later
        assert lock.acquire() is True
        lock.release()


def _subprocess_try_acquire(
    repo_path: str, locks_dir: str, result_queue: multiprocessing.Queue
) -> None:
    """Helper: try to acquire lock in a subprocess."""
    lock = InstanceLock(Path(repo_path), locks_dir=Path(locks_dir))
    acquired = lock.acquire()
    result_queue.put(acquired)
    if acquired:
        time.sleep(0.5)
        lock.release()


@pytest.mark.integration
class TestCrossProcessLocking:
    """Tests that verify locking works across OS processes.

    This is the critical behavior - we need to ensure two separate
    `kagan` processes can't run in the same repo.
    """

    def test_second_process_cannot_acquire_lock(self, tmp_path: Path) -> None:
        """A second OS process is blocked from acquiring the lock."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)
        lock.acquire()

        # Spawn a subprocess that tries to acquire the same lock
        result_queue: multiprocessing.Queue = multiprocessing.Queue()
        proc = multiprocessing.Process(
            target=_subprocess_try_acquire,
            args=(str(repo), str(locks_dir), result_queue),
        )
        proc.start()
        proc.join(timeout=5)

        subprocess_acquired = result_queue.get(timeout=2)
        assert subprocess_acquired is False, "Second process should be blocked"

        lock.release()

    def test_lock_released_when_process_exits(self, tmp_path: Path) -> None:
        """Lock becomes available when holding process exits."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        # Start a subprocess that acquires and releases the lock
        result_queue: multiprocessing.Queue = multiprocessing.Queue()
        proc = multiprocessing.Process(
            target=_subprocess_try_acquire,
            args=(str(repo), str(locks_dir), result_queue),
        )
        proc.start()
        proc.join(timeout=5)

        # Subprocess should have acquired
        first_result = result_queue.get(timeout=2)
        assert first_result is True

        # Now our process should be able to acquire (subprocess exited)
        lock = InstanceLock(repo, locks_dir=locks_dir)
        acquired = lock.acquire()
        assert acquired is True, "Lock should be available after process exits"
        lock.release()


class TestLockInfo:
    """Tests for LockInfo dataclass."""

    @pytest.mark.unit
    def test_lock_info_is_immutable(self) -> None:
        """LockInfo should be immutable (frozen dataclass)."""
        info = LockInfo(pid=123, hostname="test-host")

        with pytest.raises(AttributeError):
            info.pid = 456  # type: ignore[misc]

    @pytest.mark.unit
    def test_lock_info_contains_repo_path(self, tmp_path: Path) -> None:
        """LockInfo contains the PID, hostname, and repo path of the lock holder."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        lock = InstanceLock(repo, locks_dir=locks_dir)
        lock.acquire()

        other = InstanceLock(repo, locks_dir=locks_dir)
        info = other.get_holder_info()

        assert info is not None
        assert isinstance(info.pid, int)
        assert isinstance(info.hostname, str)
        assert info.pid == os.getpid()
        assert info.repo_path == str(repo.resolve())

        lock.release()


class TestPathCanonicalization:
    """Tests that different path representations resolve to the same lock."""

    @pytest.mark.integration
    def test_symlink_resolves_to_same_lock(self, tmp_path: Path) -> None:
        """Accessing repo via symlink uses the same lock as direct path."""
        repo = tmp_path / "actual_repo"
        repo.mkdir()
        symlink = tmp_path / "symlink_repo"
        symlink.symlink_to(repo)
        locks_dir = tmp_path / "locks"

        lock_via_real = InstanceLock(repo, locks_dir=locks_dir)
        lock_via_symlink = InstanceLock(symlink, locks_dir=locks_dir)

        # First lock via real path
        assert lock_via_real.acquire() is True

        # Trying via symlink should be blocked (same canonical path)
        assert lock_via_symlink.acquire() is False

        lock_via_real.release()

    @pytest.mark.integration
    def test_relative_and_absolute_paths_same_lock(self, tmp_path: Path) -> None:
        """Relative and absolute paths to same repo use same lock."""
        repo = tmp_path / "repo"
        repo.mkdir()
        locks_dir = tmp_path / "locks"

        # Save current dir, change to tmp_path
        original_cwd = Path.cwd()
        try:
            os.chdir(tmp_path)

            lock_absolute = InstanceLock(repo, locks_dir=locks_dir)
            lock_relative = InstanceLock(Path("repo"), locks_dir=locks_dir)

            assert lock_absolute.acquire() is True
            assert lock_relative.acquire() is False  # Same canonical path

            lock_absolute.release()
        finally:
            os.chdir(original_cwd)
