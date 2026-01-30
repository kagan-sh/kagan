"""Tests for instance lock mechanism."""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

import pytest

from kagan.lock import InstanceLock, InstanceLockError

pytestmark = pytest.mark.integration


class TestInstanceLock:
    """Tests for the InstanceLock class."""

    def test_acquire_creates_lock_file(self):
        """Test that acquiring a lock creates the lock file with PID."""
        with tempfile.TemporaryDirectory() as tmpdir:
            lock_path = Path(tmpdir) / "test.lock"
            lock = InstanceLock(lock_path)

            lock.acquire()

            assert lock_path.exists()
            assert lock_path.read_text().strip() == str(os.getpid())

            lock.release()

    def test_release_removes_lock_file(self):
        """Test that releasing a lock removes the lock file."""
        with tempfile.TemporaryDirectory() as tmpdir:
            lock_path = Path(tmpdir) / "test.lock"
            lock = InstanceLock(lock_path)

            lock.acquire()
            assert lock_path.exists()

            lock.release()
            assert not lock_path.exists()

    def test_acquire_creates_parent_directories(self):
        """Test that acquire creates parent directories if needed."""
        with tempfile.TemporaryDirectory() as tmpdir:
            lock_path = Path(tmpdir) / "subdir" / "nested" / "test.lock"
            lock = InstanceLock(lock_path)

            lock.acquire()

            assert lock_path.exists()
            lock.release()

    def test_context_manager(self):
        """Test that the lock works as a context manager."""
        with tempfile.TemporaryDirectory() as tmpdir:
            lock_path = Path(tmpdir) / "test.lock"

            with InstanceLock(lock_path) as lock:
                assert lock_path.exists()
                assert lock._acquired

            assert not lock_path.exists()

    def test_double_acquire_is_idempotent(self):
        """Test that acquiring twice doesn't cause issues."""
        with tempfile.TemporaryDirectory() as tmpdir:
            lock_path = Path(tmpdir) / "test.lock"
            lock = InstanceLock(lock_path)

            lock.acquire()
            lock.acquire()  # Should not raise

            assert lock_path.read_text().strip() == str(os.getpid())
            lock.release()

    def test_double_release_is_idempotent(self):
        """Test that releasing twice doesn't cause issues."""
        with tempfile.TemporaryDirectory() as tmpdir:
            lock_path = Path(tmpdir) / "test.lock"
            lock = InstanceLock(lock_path)

            lock.acquire()
            lock.release()
            lock.release()  # Should not raise

    def test_stale_lock_is_cleaned_up(self):
        """Test that a stale lock (dead PID) is automatically cleaned up."""
        with tempfile.TemporaryDirectory() as tmpdir:
            lock_path = Path(tmpdir) / "test.lock"

            # Write a fake PID that doesn't exist (use a very high number)
            lock_path.write_text("999999999")

            lock = InstanceLock(lock_path)
            lock.acquire()  # Should succeed, taking over the stale lock

            assert lock_path.read_text().strip() == str(os.getpid())
            lock.release()

    def test_invalid_lock_file_is_replaced(self):
        """Test that an invalid lock file is replaced."""
        with tempfile.TemporaryDirectory() as tmpdir:
            lock_path = Path(tmpdir) / "test.lock"

            # Write invalid content
            lock_path.write_text("not-a-pid")

            lock = InstanceLock(lock_path)
            lock.acquire()  # Should succeed

            assert lock_path.read_text().strip() == str(os.getpid())
            lock.release()


class TestConcurrentInstances:
    """Tests for preventing concurrent instances."""

    def test_second_instance_raises_error(self):
        """Test that a second lock attempt raises InstanceLockError."""
        with tempfile.TemporaryDirectory() as tmpdir:
            lock_path = Path(tmpdir) / "test.lock"

            # First instance acquires the lock
            lock1 = InstanceLock(lock_path)
            lock1.acquire()

            # Second instance should fail
            lock2 = InstanceLock(lock_path)
            with pytest.raises(InstanceLockError) as exc_info:
                lock2.acquire()

            assert exc_info.value.pid == os.getpid()
            assert exc_info.value.lock_path == lock_path

            lock1.release()

    def test_lock_works_across_processes(self):
        """Test that the lock prevents a real second process."""
        with tempfile.TemporaryDirectory() as tmpdir:
            lock_path = Path(tmpdir) / "test.lock"

            # Acquire lock in this process
            lock = InstanceLock(lock_path)
            lock.acquire()

            # Try to acquire in a subprocess
            result = subprocess.run(
                [
                    sys.executable,
                    "-c",
                    f"""
import sys
sys.path.insert(0, 'src')
from kagan.lock import InstanceLock, InstanceLockError
try:
    lock = InstanceLock("{lock_path}")
    lock.acquire()
    print("ACQUIRED")
except InstanceLockError as e:
    print(f"BLOCKED:{{e.pid}}")
""",
                ],
                capture_output=True,
                text=True,
                cwd=Path(__file__).parent.parent.parent,
            )

            assert "BLOCKED:" in result.stdout
            assert str(os.getpid()) in result.stdout

            lock.release()

    def test_lock_released_after_process_exit(self):
        """Test that a subprocess can acquire after parent releases."""
        with tempfile.TemporaryDirectory() as tmpdir:
            lock_path = Path(tmpdir) / "test.lock"

            # Create and release a lock
            lock = InstanceLock(lock_path)
            lock.acquire()
            lock.release()

            # Subprocess should be able to acquire
            result = subprocess.run(
                [
                    sys.executable,
                    "-c",
                    f"""
import sys
sys.path.insert(0, 'src')
from kagan.lock import InstanceLock, InstanceLockError
try:
    lock = InstanceLock("{lock_path}")
    lock.acquire()
    print("ACQUIRED")
    lock.release()
except InstanceLockError:
    print("BLOCKED")
""",
                ],
                capture_output=True,
                text=True,
                cwd=Path(__file__).parent.parent.parent,
            )

            assert "ACQUIRED" in result.stdout
