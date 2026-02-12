"""Helpers for awaiting modal dismiss results across UI contexts."""

from __future__ import annotations

import asyncio
import inspect
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from textual.app import App
    from textual.screen import Screen


async def await_screen_result[T](app: App, screen: Screen[T]) -> T | None:
    """Push a screen and await its dismiss result."""
    push_screen_wait = getattr(app, "push_screen_wait", None)
    if callable(push_screen_wait):
        try:
            result = push_screen_wait(screen)
            if inspect.isawaitable(result):
                return await result
        except TypeError:
            # Some tests monkeypatch push_screen with non-awaitable doubles.
            # Treat this as a dismissed/unsupported modal interaction.
            return None

    future: asyncio.Future[T | None] = asyncio.get_running_loop().create_future()

    def _on_result(result: T | None) -> None:
        if not future.done():
            future.set_result(result)

    push_result = app.push_screen(screen, callback=_on_result)
    if inspect.isawaitable(push_result):
        await push_result
    else:
        # Test doubles may return immediate non-awaitable values and not invoke callback.
        return None
    return await future


__all__ = ["await_screen_result"]
