from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, cast
from unittest.mock import AsyncMock

from kagan.app import KaganApp
from kagan.services.runtime import StartupSessionDecision
from kagan.ui.screens.welcome import WelcomeScreen

if TYPE_CHECKING:
    from pathlib import Path


async def test_startup_opens_project_from_runtime_session_decision(tmp_path: Path) -> None:
    app = KaganApp(db_path=":memory:", project_root=tmp_path)
    runtime_service = SimpleNamespace(
        decide_startup=AsyncMock(
            return_value=StartupSessionDecision(
                project_id="proj-persisted",
                preferred_repo_id="repo-persisted",
            )
        )
    )
    ctx = SimpleNamespace(runtime_service=runtime_service)
    app._ctx = cast("Any", ctx)

    open_project_session = AsyncMock(return_value=True)
    push_screen = AsyncMock()
    app.open_project_session = cast("Any", open_project_session)
    app.push_screen = cast("Any", push_screen)

    await app._startup_screen_decision()

    open_project_session.assert_awaited_once_with(
        "proj-persisted",
        preferred_repo_id="repo-persisted",
        preferred_path=None,
        allow_picker=False,
        screen_mode="push",
    )
    push_screen.assert_not_awaited()


async def test_startup_shows_welcome_when_no_project_decision(tmp_path: Path) -> None:
    app = KaganApp(db_path=":memory:", project_root=tmp_path)
    runtime_service = SimpleNamespace(
        decide_startup=AsyncMock(
            return_value=StartupSessionDecision(
                suggest_cwd=True,
                cwd_path=str(tmp_path),
            )
        )
    )
    ctx = SimpleNamespace(runtime_service=runtime_service)
    app._ctx = cast("Any", ctx)

    open_project_session = AsyncMock(return_value=False)
    push_screen = AsyncMock()
    app.open_project_session = cast("Any", open_project_session)
    app.push_screen = cast("Any", push_screen)

    await app._startup_screen_decision()

    open_project_session.assert_not_awaited()
    push_screen.assert_awaited_once()
    await_args = push_screen.await_args
    assert await_args is not None
    screen = await_args.args[0]
    assert isinstance(screen, WelcomeScreen)
