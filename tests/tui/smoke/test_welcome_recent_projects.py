from __future__ import annotations

from typing import TYPE_CHECKING, Any, cast

import pytest
from sqlalchemy.exc import OperationalError
from tests.helpers.config import write_test_config
from tests.helpers.git import init_git_repo_with_commit
from tests.helpers.wait import wait_for_screen, wait_until_async
from textual.widgets import ListView

from kagan.core.adapters.db.repositories import RepoRepository, TaskRepository
from kagan.core.services.projects import ProjectServiceImpl
from kagan.tui.app import KaganApp
from kagan.tui.ui.screens.welcome import WelcomeScreen

if TYPE_CHECKING:
    from pathlib import Path


async def _build_app_with_project(tmp_path: Path) -> KaganApp:
    repo_path = tmp_path / "repo"
    repo_path.mkdir()
    await init_git_repo_with_commit(repo_path)

    config_path = write_test_config(
        tmp_path / "config.toml",
        auto_review=False,
        skip_pair_instructions=True,
    )

    db_path = tmp_path / "kagan.db"
    manager = TaskRepository(db_path, project_root=tmp_path)
    await manager.initialize()
    project_id = await manager.ensure_test_project("Welcome Retry Project")

    repo_repo = RepoRepository(manager.session_factory)
    repo, _ = await repo_repo.get_or_create(repo_path, default_branch="main")
    if repo.id:
        await repo_repo.add_to_project(project_id, repo.id, is_primary=True)
    await manager.close()

    return KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=tmp_path,
    )


async def _wait_for_projects_loaded(
    screen: WelcomeScreen,
    *,
    timeout: float = 4.0,
) -> None:
    async def _has_project_rows() -> bool:
        list_view = screen.query_one("#project-list", ListView)
        return len(list_view.children) > 0

    await wait_until_async(
        _has_project_rows,
        timeout=timeout,
        check_interval=0.05,
        description="welcome project list to populate",
    )


@pytest.mark.asyncio
async def test_welcome_retries_recent_projects_after_transient_failure(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    app = await _build_app_with_project(tmp_path)
    original = ProjectServiceImpl.list_recent_projects
    calls = 0

    async def _flaky_list_recent(self: ProjectServiceImpl, limit: int = 10):
        nonlocal calls
        calls += 1
        if calls == 1:
            raise RuntimeError("Transient DB startup failure")
        return await original(self, limit)

    monkeypatch.setattr(ProjectServiceImpl, "list_recent_projects", _flaky_list_recent)

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, WelcomeScreen, timeout=10.0)
        screen = cast("WelcomeScreen", pilot.app.screen)
        await _wait_for_projects_loaded(screen)
        assert calls >= 2


@pytest.mark.asyncio
async def test_welcome_loads_projects_when_repo_lookup_fails(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    app = await _build_app_with_project(tmp_path)

    async def _failing_repo_lookup(self: ProjectServiceImpl, project_id: str):
        del self
        raise OperationalError(
            statement="SELECT repos",
            params={"project_id": project_id},
            orig=RuntimeError("database is locked"),
        )

    monkeypatch.setattr(ProjectServiceImpl, "get_project_repos", _failing_repo_lookup)

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, WelcomeScreen, timeout=10.0)
        screen = cast("WelcomeScreen", pilot.app.screen)
        await _wait_for_projects_loaded(screen)


@pytest.mark.asyncio
async def test_welcome_actions_schedule_grouped_exclusive_workers(tmp_path: Path) -> None:
    app = await _build_app_with_project(tmp_path)

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, WelcomeScreen, timeout=10.0)
        screen = cast("WelcomeScreen", pilot.app.screen)
        await _wait_for_projects_loaded(screen)

        calls: list[dict[str, object]] = []

        def _capture_run_worker(work, **kwargs):
            calls.append(kwargs)
            if hasattr(work, "close"):
                work.close()
            return None

        original_run_worker = pilot.app.run_worker
        cast("Any", pilot.app).run_worker = _capture_run_worker
        try:
            screen.action_new_project()
            screen.action_open_folder()
            screen.action_open_selected()
            screen._open_project_by_index(0)
        finally:
            cast("Any", pilot.app).run_worker = original_run_worker

        groups = {cast("str", call.get("group")) for call in calls}
        assert groups >= {
            "welcome-new-project",
            "welcome-open-folder",
            "welcome-open-selected",
            "welcome-open-project",
        }
        assert all(call.get("exclusive") is True for call in calls)
        assert all(call.get("exit_on_error") is False for call in calls)
