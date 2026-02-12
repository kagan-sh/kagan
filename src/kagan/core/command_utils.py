"""Helpers for parsing and resolving external commands across platforms."""

from __future__ import annotations

import os
import platform
import shlex
import shutil
import subprocess
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Sequence

WIN_DEFAULT_PATHEXT = ".COM;.EXE;.BAT;.CMD;.VBS;.JS;.WS;.MSC"

# Module-level cache for shutil.which() results.
# PATH doesn't change during a session, so caching avoids repeated filesystem scans.
_which_cache: dict[str, str | None] = {}


def cached_which(name: str) -> str | None:
    """Return the path for *name* using shutil.which(), with per-session caching."""
    if name not in _which_cache:
        _which_cache[name] = shutil.which(name)
    return _which_cache[name]


def clear_which_cache() -> None:
    """Clear the cached_which cache. Intended for testing."""
    _which_cache.clear()


def is_windows() -> bool:
    """Return True when running on Windows."""
    return platform.system() == "Windows"


def split_command_string(command: str) -> list[str]:
    """Parse a command string into args, using Windows-aware rules when needed."""
    if is_windows():
        try:
            import mslex  # type: ignore[import-not-found] — optional Windows-only dependency

            return mslex.split(command)
        except Exception:
            return shlex.split(command, posix=False)
    return shlex.split(command)


def resolve_command_path(command: Sequence[str]) -> list[str]:
    """Resolve the command executable to a concrete path if possible."""
    if not command:
        return []

    cmd = command[0]
    args = list(command[1:])

    if Path(cmd).name != cmd:
        return [cmd, *args]

    if is_windows():
        pathext = os.environ.get("PATHEXT", WIN_DEFAULT_PATHEXT)
        for ext in pathext.split(";"):
            potential_path = cached_which(cmd + ext)
            if potential_path:
                return [potential_path, *args]

    if resolved_cmd := cached_which(cmd):
        return [resolved_cmd, *args]

    return [cmd, *args]


def format_command_for_shell(command: str, args: Sequence[str]) -> str:
    """Format a command + args as a shell-ready string."""
    if not args:
        return command
    if is_windows():
        return subprocess.list2cmdline([command, *args])
    return f"{command} {shlex.join(list(args))}"


def ensure_windows_npm_dir() -> None:
    """Ensure the %APPDATA%/npm directory exists for npx on Windows."""
    if not is_windows():
        return

    appdata_dir = os.getenv("APPDATA")
    if not appdata_dir:
        return

    npm_dir = Path(appdata_dir).expanduser() / "npm"
    npm_dir.mkdir(parents=True, exist_ok=True)
