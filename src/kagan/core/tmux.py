"""tmux helpers for session management."""

from __future__ import annotations

from kagan.core.adapters.process import ProcessExecutionError, run_exec_checked


class TmuxError(RuntimeError):
    """Raised when tmux commands fail or tmux is not installed."""


async def run_tmux(*args: str) -> str:
    """Run a tmux command and return stdout."""
    try:
        result = await run_exec_checked("tmux", *args)
    except FileNotFoundError:
        raise TmuxError("tmux is not installed") from None
    except ProcessExecutionError as exc:
        if exc.code == "PROCESS_OS_ERROR":
            raise TmuxError("tmux is not installed") from None
        raise TmuxError(str(exc)) from exc
    return result.stdout_text().strip()
