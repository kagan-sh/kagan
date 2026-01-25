"""Instance lock to prevent multiple Kagan instances in the same folder."""

from __future__ import annotations

import os
import sys
from pathlib import Path

from kagan.constants import DEFAULT_LOCK_PATH


class InstanceLockError(Exception):
    """Raised when another Kagan instance is already running."""

    def __init__(self, pid: int, lock_path: Path):
        self.pid = pid
        self.lock_path = lock_path
        super().__init__(
            f"Another Kagan instance is already running (PID: {pid}). Lock file: {lock_path}"
        )


class InstanceLock:
    """
    File-based lock to ensure only one Kagan instance runs per directory.

    Uses a PID file approach:
    - On acquire: writes current PID to lock file
    - On release: removes lock file
    - Handles stale locks (process no longer running)
    """

    def __init__(self, lock_path: str | Path = DEFAULT_LOCK_PATH):
        self.lock_path = Path(lock_path)
        self._acquired = False

    def _is_process_running(self, pid: int) -> bool:
        """Check if a process with given PID is still running."""
        if pid <= 0:
            return False
        try:
            # On Unix, sending signal 0 checks if process exists
            os.kill(pid, 0)
            return True
        except ProcessLookupError:
            # Process does not exist
            return False
        except PermissionError:
            # Process exists but we don't have permission (still running)
            return True

    def _read_lock_pid(self) -> int | None:
        """Read PID from lock file, returns None if file doesn't exist or is invalid."""
        try:
            content = self.lock_path.read_text().strip()
            return int(content)
        except (FileNotFoundError, ValueError):
            return None

    def acquire(self) -> None:
        """
        Acquire the instance lock.

        Raises:
            InstanceLockError: If another instance is already running.
        """
        if self._acquired:
            return

        # Ensure parent directory exists
        self.lock_path.parent.mkdir(parents=True, exist_ok=True)

        # Check for existing lock
        existing_pid = self._read_lock_pid()
        if existing_pid is not None:
            if self._is_process_running(existing_pid):
                raise InstanceLockError(existing_pid, self.lock_path)
            # Stale lock file - process no longer running, we can take over

        # Write our PID
        current_pid = os.getpid()
        self.lock_path.write_text(str(current_pid))
        self._acquired = True

    def release(self) -> None:
        """Release the instance lock by removing the lock file."""
        if not self._acquired:
            return

        try:
            # Only remove if it still contains our PID
            if self._read_lock_pid() == os.getpid():
                self.lock_path.unlink(missing_ok=True)
        except OSError:
            pass  # Best effort cleanup

        self._acquired = False

    def __enter__(self) -> InstanceLock:
        self.acquire()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        self.release()


def check_single_instance(lock_path: str | Path = DEFAULT_LOCK_PATH) -> InstanceLock:
    """
    Convenience function to check and acquire instance lock.

    Returns the lock object (caller should release on exit).

    Raises:
        InstanceLockError: If another instance is already running.
    """
    lock = InstanceLock(lock_path)
    lock.acquire()
    return lock


def exit_if_already_running(lock_path: str | Path = DEFAULT_LOCK_PATH) -> InstanceLock:
    """
    Check for another instance and exit with error message if found.

    Returns the acquired lock on success.
    """
    lock = InstanceLock(lock_path)
    try:
        lock.acquire()
        return lock
    except InstanceLockError as e:
        print(f"Error: {e}", file=sys.stderr)
        print(
            "If no other instance is running, remove the lock file manually:",
            file=sys.stderr,
        )
        print(f"  rm {e.lock_path}", file=sys.stderr)
        sys.exit(1)
