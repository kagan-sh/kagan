"""Core utility helpers: background tasks and queue truncation."""

from __future__ import annotations

import asyncio
import contextlib
import logging
from typing import TYPE_CHECKING, Any, TypeVar

if TYPE_CHECKING:
    from collections.abc import Coroutine

_T = TypeVar("_T")
log = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Queue truncation
# ---------------------------------------------------------------------------

DEFAULT_QUEUE_TRUNCATION_PREFIX = "[queued context truncated]\n"
DEFAULT_QUEUE_TRUNCATION_MAX_CHARS = 8000


def truncate_queue_payload(
    content: str,
    *,
    prefix: str = DEFAULT_QUEUE_TRUNCATION_PREFIX,
    max_chars: int = DEFAULT_QUEUE_TRUNCATION_MAX_CHARS,
) -> str:
    """Trim oversized queued content and keep the newest tail."""
    if max_chars <= 0:
        return ""
    if len(content) <= max_chars:
        return content
    if max_chars <= len(prefix):
        return content[-max_chars:]
    return f"{prefix}{content[-(max_chars - len(prefix)) :]}"


# ---------------------------------------------------------------------------
# Background tasks
# ---------------------------------------------------------------------------


class BackgroundTasks:
    """Track lightweight background tasks and shut them down gracefully."""

    def __init__(self) -> None:
        self._tasks: set[asyncio.Task[Any]] = set()

    def register(self, task: asyncio.Task[_T]) -> asyncio.Task[_T]:
        """Register an existing task and remove it once it completes."""
        self._tasks.add(task)

        def _on_done(done_task: asyncio.Task[object]) -> None:
            self._tasks.discard(done_task)
            if done_task.cancelled():
                return
            with contextlib.suppress(asyncio.CancelledError):
                exc = done_task.exception()
            if exc is None:
                return
            log.error(
                "Background task failed",
                extra={"task_name": done_task.get_name()},
                exc_info=(type(exc), exc, exc.__traceback__),
            )

        task.add_done_callback(_on_done)
        return task

    def spawn(
        self,
        coro: Coroutine[Any, Any, _T],
        *,
        name: str | None = None,
    ) -> asyncio.Task[_T]:
        """Create and register a background task."""
        return self.register(asyncio.create_task(coro, name=name))

    async def shutdown(self, *, timeout: float = 2.0) -> None:
        """Cancel tracked tasks and wait briefly for graceful completion."""
        pending = [task for task in self._tasks if not task.done()]
        if not pending:
            return

        for task in pending:
            task.cancel()

        done, _pending = await asyncio.wait(pending, timeout=timeout)
        if done:
            await asyncio.gather(*done, return_exceptions=True)
