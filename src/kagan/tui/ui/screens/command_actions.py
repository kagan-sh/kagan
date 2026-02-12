from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from textual.css.query import NoMatches

if TYPE_CHECKING:
    from textual.screen import Screen


def screen_allows_action(
    screen: Screen,
    action: str,
    *,
    dispatch_method: str | None = None,
) -> bool:
    """Check whether a screen action is currently available."""
    dispatch = getattr(screen, dispatch_method, None) if dispatch_method is not None else None
    if not callable(dispatch) and not hasattr(screen, f"action_{action}"):
        return False

    check_action = getattr(screen, "check_action", None)
    if check_action is None:
        return True

    try:
        return check_action(action, ()) is True
    except (NoMatches, AttributeError, RuntimeError):
        return False


async def run_screen_action(
    screen: Screen,
    action: str,
    *,
    dispatch_method: str | None = None,
) -> None:
    """Run action via dispatcher method when present, else direct action method."""
    dispatch = getattr(screen, dispatch_method, None) if dispatch_method is not None else None
    if callable(dispatch):
        result = dispatch(action)
        if asyncio.iscoroutine(result):
            await result
        return

    action_method = getattr(screen, f"action_{action}", None)
    if action_method is None:
        return

    result = action_method()
    if asyncio.iscoroutine(result):
        await result
