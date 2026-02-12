from __future__ import annotations

import os
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any, cast
from unittest.mock import AsyncMock, MagicMock, Mock

from kagan.core.adapters.db.schema import Repo
from kagan.core.instance_lock import LockInfo
from kagan.core.services.runtime import RuntimeSessionEvent, StartupSessionDecision
from kagan.tui.app import KaganApp
from kagan.tui.ui.screens.welcome import WelcomeScreen

if TYPE_CHECKING:
    from pathlib import Path


async def test_try_switch_lock_treats_same_pid_holder_as_current_instance(
    monkeypatch,
    tmp_path: Path,
) -> None:
    app = KaganApp(db_path=":memory:", project_root=tmp_path / "repo-a")
    current_lock = SimpleNamespace(release=Mock())
    app._instance_lock = cast("Any", current_lock)

    class SamePidLock:
        def __init__(self, _repo_path: Path) -> None:
            pass

        def acquire(self) -> bool:
            return False

        def get_holder_info(self) -> LockInfo:
            return LockInfo(pid=os.getpid(), hostname="local", repo_path=str(tmp_path / "repo-b"))

    monkeypatch.setattr("kagan.tui.app.InstanceLock", SamePidLock)

    result = await app._try_switch_lock(tmp_path / "repo-b")

    assert result is None
    current_lock.release.assert_not_called()
    assert app._instance_lock is current_lock


async def test_try_switch_lock_returns_holder_for_other_process(
    monkeypatch,
    tmp_path: Path,
) -> None:
    app = KaganApp(db_path=":memory:", project_root=tmp_path / "repo-a")
    current_lock = SimpleNamespace(release=Mock())
    app._instance_lock = cast("Any", current_lock)

    class OtherPidLock:
        def __init__(self, _repo_path: Path) -> None:
            pass

        def acquire(self) -> bool:
            return False

        def get_holder_info(self) -> LockInfo:
            return LockInfo(pid=os.getpid() + 1000, hostname="other", repo_path=None)

    monkeypatch.setattr("kagan.tui.app.InstanceLock", OtherPidLock)

    result = await app._try_switch_lock(tmp_path / "repo-b")

    assert result is not None
    assert result.hostname == "other"
    current_lock.release.assert_not_called()


async def test_startup_opens_project_from_runtime_session_decision(tmp_path: Path) -> None:
    app = KaganApp(db_path=":memory:", project_root=tmp_path)
    decide_mock = AsyncMock(
        return_value=StartupSessionDecision(
            project_id="proj-persisted",
            preferred_repo_id="repo-persisted",
        )
    )
    runtime_service = SimpleNamespace(decide_startup=decide_mock)
    api = SimpleNamespace(decide_startup=decide_mock)
    ctx = SimpleNamespace(runtime_service=runtime_service, api=api)
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
    decide_mock = AsyncMock(
        return_value=StartupSessionDecision(
            suggest_cwd=True,
            cwd_path=str(tmp_path),
        )
    )
    runtime_service = SimpleNamespace(decide_startup=decide_mock)
    api = SimpleNamespace(decide_startup=decide_mock)
    ctx = SimpleNamespace(runtime_service=runtime_service, api=api)
    app._ctx = cast("Any", ctx)

    open_project_session = AsyncMock(return_value=False)
    push_screen = AsyncMock()
    app.open_project_session = cast("Any", open_project_session)
    app.push_screen = cast("Any", push_screen)

    await app._startup_screen_decision()

    open_project_session.assert_not_awaited()
    push_screen.assert_awaited_once()
    screen = push_screen.await_args.args[0]
    assert isinstance(screen, WelcomeScreen)


async def test_apply_active_repo_bootstraps_session_service_when_core_connected(
    tmp_path: Path,
) -> None:
    app = KaganApp(db_path=":memory:", project_root=tmp_path)
    api = SimpleNamespace(bootstrap_session_service=MagicMock())
    ctx = SimpleNamespace(active_project_id="proj-1", api=api)
    app._ctx = cast("Any", ctx)
    app._instance_lock = None
    app._core_client = cast("Any", object())

    dispatch_runtime = AsyncMock()
    app._dispatch_runtime_session = cast("Any", dispatch_runtime)

    repo_path = tmp_path / "new-repo"
    repo = Repo(name="new-repo", path=str(repo_path))

    opened = await app._apply_active_repo(repo, project_id="proj-1")

    assert opened is True
    assert app.project_root == repo_path
    dispatch_runtime.assert_awaited_once_with(
        RuntimeSessionEvent.REPO_SELECTED,
        project_id="proj-1",
        repo_id=repo.id,
    )
    api.bootstrap_session_service.assert_called_once_with(repo_path, app.config)
