from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, cast

import pytest
from tests.helpers.git import init_git_repo_with_commit
from tests.helpers.wait import wait_for_screen
from textual.widgets import ListView

from kagan.adapters.db.repositories import RepoRepository, TaskRepository
from kagan.app import KaganApp
from kagan.services.projects import ProjectServiceImpl
from kagan.ui.screens.welcome import WelcomeScreen

if TYPE_CHECKING:
    from pathlib import Path


async def _build_app_with_project(tmp_path: Path) -> KaganApp:
    repo_path = tmp_path / "repo"
    repo_path.mkdir()
    await init_git_repo_with_commit(repo_path)

    config_path = tmp_path / "config.toml"
    config_path.write_text(
        """[general]
auto_review = false
default_base_branch = "main"
default_worker_agent = "claude"

[ui]
skip_pair_instructions = true

[agents.claude]
identity = "claude.ai"
name = "Claude"
short_name = "claude"
run_command."*" = "echo mock-claude"
interactive_command."*" = "echo mock-claude-interactive"
active = true
""",
        encoding="utf-8",
    )

    db_path = tmp_path / "kagan.db"
    manager = TaskRepository(db_path, project_root=tmp_path)
    await manager.initialize()
    project_id = await manager.ensure_test_project("Welcome Retry Project")

    assert manager._session_factory is not None
    repo_repo = RepoRepository(manager._session_factory)
    repo, _ = await repo_repo.get_or_create(repo_path, default_branch="main")
    if repo.id:
        await repo_repo.add_to_project(project_id, repo.id, is_primary=True)
    await manager.close()

    return KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=tmp_path,
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

        loaded = False
        for _ in range(80):
            await pilot.pause()
            list_view = screen.query_one("#project-list", ListView)
            if len(list_view.children) > 0:
                loaded = True
                break
            await asyncio.sleep(0.05)

        assert loaded
        assert calls >= 2


@pytest.mark.asyncio
async def test_welcome_loads_projects_when_repo_lookup_fails(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    app = await _build_app_with_project(tmp_path)

    async def _failing_repo_lookup(self: ProjectServiceImpl, project_id: str):
        del self, project_id
        raise Exception("database is locked")

    monkeypatch.setattr(ProjectServiceImpl, "get_project_repos", _failing_repo_lookup)

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, WelcomeScreen, timeout=10.0)
        screen = cast("WelcomeScreen", pilot.app.screen)

        loaded = False
        for _ in range(80):
            await pilot.pause()
            list_view = screen.query_one("#project-list", ListView)
            if len(list_view.children) > 0:
                loaded = True
                break
            await asyncio.sleep(0.05)

        assert loaded
