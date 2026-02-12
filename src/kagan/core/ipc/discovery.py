"""Endpoint discovery for a running Kagan core process."""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from kagan.core.paths import (
    get_core_endpoint_path,
    get_core_runtime_dir,
    get_core_token_path,
)

if TYPE_CHECKING:
    from pathlib import Path

logger = logging.getLogger(__name__)


@dataclass(frozen=True)
class CoreEndpoint:
    """Describes how to connect to a running core instance.

    Attributes:
        transport: The transport type (``socket`` or ``tcp``).
        address: The connection address (file path for socket, host for tcp).
        port: TCP port when *transport* is ``tcp``; ``None`` for socket transport.
        pid: OS process ID of the core, used for liveness checks.
        token: Bearer token for authenticating IPC requests.
    """

    transport: str
    address: str
    port: int | None = None
    pid: int | None = None
    token: str | None = None


def _read_json(path: Path) -> dict[str, Any] | None:
    """Read and parse a JSON file, returning *None* on any failure."""
    try:
        text = path.read_text(encoding="utf-8")
        data = json.loads(text)
        if isinstance(data, dict):
            return data
        return None
    except (OSError, json.JSONDecodeError, ValueError):
        return None


def _read_text(path: Path) -> str | None:
    """Read a text file and return its stripped contents, or *None*."""
    try:
        return path.read_text(encoding="utf-8").strip() or None
    except OSError:
        return None


def _read_pid_from_lease() -> int | None:
    """Read owner PID from the core lease file, when available."""
    lease = _read_json(get_core_runtime_dir() / "core.lease.json")
    if not isinstance(lease, dict):
        return None
    raw_owner_pid = lease.get("owner_pid")
    if isinstance(raw_owner_pid, int):
        owner_pid = raw_owner_pid
    elif isinstance(raw_owner_pid, str):
        try:
            owner_pid = int(raw_owner_pid)
        except ValueError:
            return None
    else:
        return None
    return owner_pid if owner_pid > 0 else None


def _is_process_alive(pid: int) -> bool:
    """Check whether a process with the given PID is alive."""
    try:
        import psutil

        return psutil.pid_exists(pid)
    except Exception:
        # Fallback: POSIX signal-0 check
        import os

        try:
            os.kill(pid, 0)
            return True
        except (OSError, ProcessLookupError):
            return False


def discover_core_endpoint() -> CoreEndpoint | None:
    """Discover a running Kagan core by reading runtime files.

    Returns a ``CoreEndpoint`` when a live core is found, or ``None`` if
    no endpoint file exists, the file is invalid, or the referenced process
    is no longer running.
    """
    endpoint_path = get_core_endpoint_path()
    data = _read_json(endpoint_path)
    if data is None:
        logger.debug("No endpoint file at %s", endpoint_path)
        return None

    transport = data.get("transport")
    address = data.get("address")
    if not transport or not address:
        logger.warning("Malformed endpoint file at %s", endpoint_path)
        return None
    normalized_transport = str(transport)
    if normalized_transport not in {"socket", "tcp"}:
        logger.warning(
            "Unsupported transport '%s' in endpoint file at %s",
            transport,
            endpoint_path,
        )
        return None

    pid = _read_pid_from_lease()
    if pid is not None and not _is_process_alive(pid):
        logger.info("Core process (PID %d) is no longer running; stale endpoint", pid)
        return None

    token = _read_text(get_core_token_path())

    return CoreEndpoint(
        transport=normalized_transport,
        address=str(address),
        port=data.get("port"),
        pid=pid,
        token=token,
    )


__all__ = [
    "CoreEndpoint",
    "discover_core_endpoint",
]
