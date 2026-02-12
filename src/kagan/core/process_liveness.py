"""Cross-platform process liveness checks."""

from __future__ import annotations

import os


def _pid_exists_windows(pid: int) -> bool:
    import ctypes

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    access = 0x1000  # PROCESS_QUERY_LIMITED_INFORMATION
    handle = kernel32.OpenProcess(access, False, pid)
    if handle:
        kernel32.CloseHandle(handle)
        return True

    # Access denied implies the process exists but cannot be queried.
    return ctypes.get_last_error() == 5  # ERROR_ACCESS_DENIED


def _pid_exists_psutil(pid: int) -> bool | None:
    try:
        import psutil

        return psutil.pid_exists(pid)
    except Exception:
        return None


def pid_exists(pid: int) -> bool:
    """Return whether *pid* appears to refer to a live process."""
    if pid <= 0:
        return False
    psutil_result = _pid_exists_psutil(pid)
    if psutil_result is not None:
        return psutil_result

    if os.name == "nt":
        return _pid_exists_windows(pid)

    try:
        os.kill(pid, 0)
        return True
    except PermissionError:
        return True
    except OSError:
        return False
