"""Snapshot tests for RepoPickerScreen."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Any, cast

import pytest

from kagan.ui.screens.repo_picker import RepoPickerScreen
from tests.helpers.git import init_git_repo_with_commit
from tests.helpers.mocks import create_fake_tmux

if TYPE_CHECKING:
    from types import SimpleNamespace

    from textual.pilot import Pilot

    from tests.snapshots.conftest import MockAgentFactory


class TestRepoPickerScreen:
    @pytest.mark.snapshot
    def test_repo_picker_with_multiple_repos(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """RepoPickerScreen shows a list of repos for selection."""
        from kagan.adapters.db.repositories import RepoRepository, TaskRepository
        from kagan.app import KaganApp

        sessions: dict[str, Any] = {}
        fake_tmux = create_fake_tmux(sessions)
        monkeypatch.setattr("kagan.tmux.run_tmux", fake_tmux)
        monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_tmux)

        app = KaganApp(
            db_path=snapshot_project.db,
            config_path=snapshot_project.config,
            project_root=snapshot_project.root,
            agent_factory=mock_acp_agent_factory,
        )

        async def run_before(pilot: Pilot) -> None:
            await pilot.pause()

            extra_repo = Path(snapshot_project.root).parent / "snapshot_repo_two"
            extra_repo.mkdir()
            await init_git_repo_with_commit(extra_repo)

            task_repo = TaskRepository(snapshot_project.db, project_root=snapshot_project.root)
            await task_repo.initialize()
            project_id = await task_repo.ensure_test_project("Snapshot Test Project")
            assert task_repo._session_factory is not None
            repo_repo = RepoRepository(task_repo._session_factory)
            repo, _ = await repo_repo.get_or_create(extra_repo, default_branch="main")
            if repo.id:
                await repo_repo.add_to_project(project_id, repo.id, is_primary=False)
            await task_repo.close()

            app = cast("KaganApp", pilot.app)
            project = await app.ctx.project_service.open_project(project_id)
            repos = await app.ctx.project_service.get_project_repos(project_id)

            await app.push_screen(
                RepoPickerScreen(project=project, repositories=repos, current_repo_id=repos[0].id)
            )
            await pilot.pause()

        cols, rows = snapshot_terminal_size
        assert snap_compare(app, terminal_size=(cols, rows), run_before=run_before)
