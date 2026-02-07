"""tmux helpers for session management."""

from __future__ import annotations

import asyncio


class TmuxError(RuntimeError):
    """Raised when tmux commands fail or tmux is not installed."""


async def run_tmux(*args: str) -> str:
    """Run a tmux command and return stdout."""
    try:
        process = await asyncio.create_subprocess_exec(
            "tmux",
            *args,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
    except FileNotFoundError:
        raise TmuxError("tmux is not installed") from None
    stdout, stderr = await process.communicate()
    if process.returncode != 0:
        raise TmuxError(stderr.decode().strip() or "tmux command failed")
    return stdout.decode().strip()
