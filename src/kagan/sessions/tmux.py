"""tmux helpers for session management."""

from __future__ import annotations

import asyncio


class TmuxError(RuntimeError):
    """Raised when tmux commands fail."""


async def run_tmux(*args: str) -> str:
    """Run a tmux command and return stdout."""
    process = await asyncio.create_subprocess_exec(
        "tmux",
        *args,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    stdout, stderr = await process.communicate()
    if process.returncode != 0:
        raise TmuxError(stderr.decode().strip() or "tmux command failed")
    return stdout.decode().strip()
