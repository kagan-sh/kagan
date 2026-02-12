"""Core process launcher — discovers, starts, and supervises the core daemon."""

from __future__ import annotations

import asyncio
import contextlib
import json
import logging
import os
import subprocess
import sys
import time
from typing import TYPE_CHECKING

from kagan.core.adapters.process import spawn_detached
from kagan.core.ipc.discovery import CoreEndpoint, discover_core_endpoint
from kagan.core.paths import (
    get_config_path,
    get_core_runtime_dir,
    get_database_path,
)

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.core.config import KaganConfig

logger = logging.getLogger(__name__)

_CORE_START_LOCK_NAME = "core.start.lock"
_CORE_START_POLL_SECONDS = 0.2
_CORE_START_LOCK_STALE_SECONDS = 60.0


def _build_daemon_command(config_path: Path, db_path: Path) -> list[str]:
    """Build a Python command that runs the dedicated core daemon module."""
    return [
        sys.executable,
        "-m",
        "kagan.core.daemon",
        "--config-path",
        str(config_path),
        "--db-path",
        str(db_path),
    ]


def _spawn_core_detached(*, config_path: Path, db_path: Path) -> subprocess.Popen[bytes]:
    """Start the core daemon in a detached subprocess and return the process handle."""
    cmd = _build_daemon_command(config_path, db_path)

    if os.name == "nt":
        # Keep the core alive independently from the launching terminal process.
        creationflags = 0
        creationflags |= getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
        creationflags |= getattr(subprocess, "CREATE_NO_WINDOW", 0)
        return spawn_detached(cmd, windows_creationflags=creationflags)

    return spawn_detached(cmd)


def _core_start_lock_path() -> Path:
    return get_core_runtime_dir() / _CORE_START_LOCK_NAME


def _try_acquire_start_lock(lock_path: Path) -> bool:
    try:
        fd = os.open(lock_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError:
        return False
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        handle.write(f"{os.getpid()}\n")
    return True


def _release_start_lock(lock_path: Path) -> None:
    with contextlib.suppress(FileNotFoundError):
        lock_path.unlink()


def _maybe_clear_stale_start_lock(lock_path: Path, *, stale_after_seconds: float) -> None:
    try:
        lock_age = time.time() - lock_path.stat().st_mtime
    except FileNotFoundError:
        return
    if lock_age < stale_after_seconds:
        return
    logger.warning("Removing stale core start lock older than %.1fs", lock_age)
    with contextlib.suppress(FileNotFoundError):
        lock_path.unlink()


def _read_pid(path: Path) -> int | None:
    try:
        raw = path.read_text(encoding="utf-8").strip()
    except OSError:
        return None
    if not raw:
        return None
    try:
        return int(raw)
    except ValueError:
        return None


def _pid_exists(pid: int) -> bool:
    try:
        import psutil

        return psutil.pid_exists(pid)
    except Exception:  # quality-allow-broad-except
        try:
            os.kill(pid, 0)
            return True
        except OSError:
            return False


def _has_live_core_instance_lock() -> bool:
    pid: int | None = None
    lease_path = get_core_runtime_dir() / "core.lease.json"
    try:
        lease_data = json.loads(lease_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError, TypeError, ValueError):
        lease_data = None
    if isinstance(lease_data, dict):
        raw_owner_pid = lease_data.get("owner_pid")
        if isinstance(raw_owner_pid, int):
            pid = raw_owner_pid
        elif isinstance(raw_owner_pid, str):
            with contextlib.suppress(ValueError):
                pid = int(raw_owner_pid)
    if pid is None:
        pid = _read_pid(get_core_runtime_dir() / "core.instance.lock")
    return pid is not None and _pid_exists(pid)


async def ensure_core_running(
    *,
    config: KaganConfig | None = None,
    config_path: Path | None = None,
    db_path: Path | None = None,
    timeout: float = 15.0,
) -> CoreEndpoint:
    """Ensure a core host is running, starting one if necessary.

    1. Try to discover an existing core.
    2. If none found, start a new detached core daemon subprocess.
    3. Wait up to *timeout* seconds for the endpoint to become available.

    Returns:
        A ``CoreEndpoint`` describing the running core.

    Raises:
        TimeoutError: If the core does not become available within *timeout*.
    """
    # Check for existing core
    endpoint = discover_core_endpoint()
    if endpoint is not None:
        logger.info("Found existing core: %s %s", endpoint.transport, endpoint.address)
        return endpoint

    logger.info("No running core found, starting one...")
    del config  # The daemon process loads config from *config_path*.
    config_path = config_path or get_config_path()
    db_path = db_path or get_database_path()
    runtime_dir = get_core_runtime_dir()
    runtime_dir.mkdir(parents=True, exist_ok=True)
    lock_path = _core_start_lock_path()
    process: subprocess.Popen[bytes] | None = None
    has_start_lock = False

    # Poll for endpoint availability
    deadline = asyncio.get_running_loop().time() + timeout
    stale_after = max(_CORE_START_LOCK_STALE_SECONDS, timeout * 2)
    try:
        while asyncio.get_running_loop().time() < deadline:
            endpoint = discover_core_endpoint()
            if endpoint is not None:
                return endpoint

            if not has_start_lock:
                has_start_lock = _try_acquire_start_lock(lock_path)
                if has_start_lock:
                    process = _spawn_core_detached(config_path=config_path, db_path=db_path)
                else:
                    _maybe_clear_stale_start_lock(lock_path, stale_after_seconds=stale_after)

            if process is not None and process.poll() is not None:
                # A concurrent launcher may already be bringing core up and still
                # writing runtime discovery files. In that case, keep waiting.
                if _has_live_core_instance_lock():
                    process = None
                else:
                    msg = f"Core daemon exited early with code {process.returncode}"
                    raise RuntimeError(msg)

            await asyncio.sleep(_CORE_START_POLL_SECONDS)
    finally:
        if has_start_lock:
            _release_start_lock(lock_path)

    msg = f"Core host did not become available within {timeout}s"
    raise TimeoutError(msg)


def ensure_core_running_sync(
    *,
    config: KaganConfig | None = None,
    config_path: Path | None = None,
    db_path: Path | None = None,
    timeout: float = 15.0,
) -> CoreEndpoint:
    """Synchronous wrapper for ``ensure_core_running`` for Click commands."""
    try:
        asyncio.get_running_loop()
    except RuntimeError:
        return asyncio.run(
            ensure_core_running(
                config=config,
                config_path=config_path,
                db_path=db_path,
                timeout=timeout,
            )
        )
    msg = "ensure_core_running_sync cannot be called from within a running event loop"
    raise RuntimeError(msg)


def launch_core_subprocess(
    *,
    config_path: Path | None = None,
    db_path: Path | None = None,
) -> int:
    """Launch the core host as a blocking process (used by ``kagan core start``).

    Returns:
        Exit code (0 for clean shutdown).
    """
    from kagan.core.host import CoreHost

    config_path = config_path or get_config_path()
    db_path = db_path or get_database_path()

    async def _run() -> None:
        host = CoreHost(config_path=config_path, db_path=db_path)
        await host.start()
        await host.wait_until_stopped()

    try:
        asyncio.run(_run())
    except KeyboardInterrupt:
        logger.info("Core host interrupted by user")
    return 0


__all__ = [
    "ensure_core_running",
    "ensure_core_running_sync",
    "launch_core_subprocess",
]
