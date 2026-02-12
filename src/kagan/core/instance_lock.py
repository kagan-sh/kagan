"""Per-repository instance locking to prevent concurrent Kagan instances.

Lock files are stored in XDG state directory (~/.local/state/kagan/locks/)
to avoid polluting user repositories with lock files.
"""

from __future__ import annotations

import contextlib
import hashlib
import os
import socket
from dataclasses import dataclass
from pathlib import Path

from filelock import FileLock, Timeout

LOCKS_DIR_NAME = "locks"


def _get_locks_dir() -> Path:
    """Get the directory for storing lock files.

    Uses XDG_STATE_HOME/kagan/locks/ if XDG_STATE_HOME is set,
    otherwise defaults to ~/.local/state/kagan/locks/.
    """
    xdg_state = os.environ.get("XDG_STATE_HOME")
    # Use XDG_STATE_HOME if set, otherwise default XDG state location
    base = Path(xdg_state) / "kagan" if xdg_state else Path.home() / ".local" / "state" / "kagan"
    return base / LOCKS_DIR_NAME


def _hash_path(repo_root: Path) -> str:
    """Create a deterministic hash from a repo path.

    Uses the resolved (canonical) path to ensure consistency
    regardless of how the path is specified (symlinks, relative, etc.).

    Note: Uses 16 hex chars (64 bits) which has collision probability ~1/2^32
    (birthday bound). Acceptable for typical use case (<1000 repos per user).
    """
    canonical = str(repo_root.resolve())
    return hashlib.sha256(canonical.encode()).hexdigest()[:16]


@dataclass(frozen=True, slots=True)
class LockInfo:
    """Information about the process holding the lock."""

    pid: int
    hostname: str
    repo_path: str | None = None


class InstanceLock:
    """File-based lock for single-instance-per-repo enforcement.

    Uses filelock for cross-platform advisory locking. The lock is
    automatically released when the process exits (including crashes).

    Lock files are stored in XDG state directory to avoid polluting
    the user's repository with lock files.

    Note: Holder info is stored in a separate .info file because filelock
    truncates the lock file on acquire attempts (even failed ones).

    Usage:
        lock = InstanceLock(repo_root)
        if not lock.acquire():
            info = lock.get_holder_info()
            show_modal(f"Locked by PID {info.pid} on {info.repo_path}")
            return
        # Lock auto-released on exit, or call lock.release()
    """

    def __init__(self, repo_root: Path, *, locks_dir: Path | None = None) -> None:
        """Initialize instance lock.

        Args:
            repo_root: The repository root to lock.
            locks_dir: Override lock directory (for testing). If None, uses XDG state dir.
        """
        self._repo_root = repo_root.resolve()
        self._locks_dir = locks_dir if locks_dir is not None else _get_locks_dir()
        self._locks_dir.mkdir(parents=True, exist_ok=True)

        path_hash = _hash_path(repo_root)
        self._lock_path = self._locks_dir / f"{path_hash}.lock"
        self._info_path = self._locks_dir / f"{path_hash}.info"
        self._lock = FileLock(str(self._lock_path), blocking=False)
        self._acquired = False

    def acquire(self, *, _retry_stale: bool = True) -> bool:
        """Attempt to acquire the lock (non-blocking).

        Returns True if lock acquired, False if another instance holds it.
        """
        if self._acquired:
            return True

        try:
            self._lock.acquire(timeout=0)
            self._write_holder_info()
            self._acquired = True
            return True
        except Timeout:
            if _retry_stale:
                holder = self.get_holder_info()
                if holder is not None and self._is_stale_holder(holder):
                    self._cleanup_stale_lock_files()
                    self._lock = FileLock(str(self._lock_path), blocking=False)
                    return self.acquire(_retry_stale=False)
            return False

    @staticmethod
    def _pid_is_running(pid: int) -> bool:
        if pid <= 0:
            return False
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        except PermissionError:
            return True
        except OSError:
            return False
        return True

    def _is_stale_holder(self, holder: LockInfo) -> bool:
        if holder.pid == os.getpid():
            return False
        if holder.hostname not in {"", "unknown", socket.gethostname()}:
            return False
        return not self._pid_is_running(holder.pid)

    def _cleanup_stale_lock_files(self) -> None:
        with contextlib.suppress(OSError):
            self._lock_path.unlink(missing_ok=True)
        with contextlib.suppress(OSError):
            self._info_path.unlink(missing_ok=True)

    def release(self) -> None:
        """Release the lock if held."""
        if not self._acquired:
            return

        try:
            self._lock.release()
            # Clean up lock files (best effort)
            self._lock_path.unlink(missing_ok=True)
            self._info_path.unlink(missing_ok=True)
        except OSError:
            pass

        self._acquired = False

    def _write_holder_info(self) -> None:
        """Write process info to separate info file.

        Stored separately because filelock truncates the lock file
        on acquire attempts, even when the lock is already held.

        Note: If the process crashes between lock acquisition and this write,
        the lock is still held but get_holder_info() will return None.
        This is acceptable as the lock mechanism still works correctly.
        """
        # Best effort write - lock still works even if this fails
        with contextlib.suppress(OSError):
            self._info_path.write_text(
                f"{os.getpid()}\n{socket.gethostname()}\n{self._repo_root}\n"
            )

    def get_holder_info(self) -> LockInfo | None:
        """Read info about the process holding the lock."""
        try:
            content = self._info_path.read_text().strip().split("\n")
            if len(content) >= 3:
                return LockInfo(pid=int(content[0]), hostname=content[1], repo_path=content[2])
            if len(content) >= 2:
                return LockInfo(pid=int(content[0]), hostname=content[1])
            if len(content) == 1:
                return LockInfo(pid=int(content[0]), hostname="unknown")
        except (OSError, ValueError):
            pass
        return None

    @property
    def is_held(self) -> bool:
        """Return True if we hold the lock."""
        return self._acquired

    def __enter__(self) -> InstanceLock:
        """Context manager entry."""
        if not self.acquire():
            raise Timeout(str(self._lock_path))
        return self

    def __exit__(self, *args: object) -> None:
        """Context manager exit."""
        self.release()
