"""Terminal backend installer utilities."""

from __future__ import annotations

import asyncio
import platform
import shutil

INSTALL_TIMEOUT_SECONDS = 180
WINDOWS_FALLBACK_ORDER = ("vscode", "cursor")
UNIX_FALLBACK_ORDER = ("vscode", "cursor")


def _which(command: str) -> str | None:
    """Resolve executable path via stdlib lookup.

    Keeping `shutil.which` reachable preserves test monkeypatch compatibility.
    """
    return shutil.which(command)


def first_available_pair_backend(*, windows: bool) -> str | None:
    """Return the first available fallback backend in priority order."""
    order = WINDOWS_FALLBACK_ORDER if windows else UNIX_FALLBACK_ORDER
    for backend in order:
        if check_terminal_installed(backend):
            return backend
    return None


def check_terminal_installed(backend: str) -> bool:
    """Return whether the requested terminal backend executable exists in PATH."""
    executable_map = {
        "tmux": "tmux",
        "vscode": "code",
        "cursor": "cursor",
    }
    executable = executable_map.get(backend)
    if executable is None:
        return False
    return _which(executable) is not None


def get_manual_install_fallback(backend: str) -> str:
    """Return concise manual installation guidance for a terminal backend."""
    system = platform.system()

    if backend == "tmux":
        if system == "Darwin":
            return (
                "Install tmux: brew install tmux. "
                "Or use VS Code/Cursor: https://code.visualstudio.com/download "
                "https://cursor.com/downloads"
            )
        if system == "Linux":
            return (
                "Install tmux from your package manager. "
                "Or use VS Code/Cursor: https://code.visualstudio.com/download "
                "https://cursor.com/downloads"
            )
        return "Install tmux manually and retry."
    if backend == "vscode":
        return "Install VS Code: https://code.visualstudio.com/download"
    if backend == "cursor":
        return "Install Cursor: https://cursor.com/downloads"
    return "Install the selected terminal backend manually and retry."


def _get_tmux_install_command() -> str | None:
    system = platform.system()

    if system == "Darwin":
        if _which("brew") is None:
            return None
        return "brew install tmux"

    if system == "Linux":
        if _which("apt-get") is not None:
            return "sudo apt-get update && sudo apt-get install -y tmux"
        if _which("dnf") is not None:
            return "sudo dnf install -y tmux"
        if _which("pacman") is not None:
            return "sudo pacman -S --noconfirm tmux"
        if _which("zypper") is not None:
            return "sudo zypper --non-interactive install tmux"
        return None

    return None


async def install_terminal(
    backend: str,
    *,
    timeout: float = INSTALL_TIMEOUT_SECONDS,
) -> tuple[bool, str]:
    """Best-effort terminal install flow.

    Returns:
        (success, message)
    """
    if backend != "tmux":
        return False, "Automatic install is supported only for tmux."

    if check_terminal_installed(backend):
        return True, f"{backend} is already installed."

    install_cmd = _get_tmux_install_command()
    manual_fallback = get_manual_install_fallback(backend)
    if install_cmd is None:
        return False, f"Could not detect an automatic installer. {manual_fallback}"

    try:
        proc = await asyncio.create_subprocess_shell(
            install_cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        try:
            _stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout=timeout)
        except TimeoutError:
            proc.kill()
            await proc.wait()
            return False, f"Install timed out. {manual_fallback}"

        if proc.returncode == 0 and check_terminal_installed(backend):
            return True, f"{backend} installed successfully."

        stderr_text = stderr.decode(errors="ignore").strip()
        if stderr_text:
            return False, f"Install failed: {stderr_text}. {manual_fallback}"
        return False, f"Install did not complete successfully. {manual_fallback}"

    except OSError as exc:
        return False, f"Install failed: {exc}. {manual_fallback}"
