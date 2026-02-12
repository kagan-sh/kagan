"""Snapshot tests for RepoPickerScreen."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Any, cast

import pytest
from tests.helpers.git import init_git_repo_with_commit

from kagan.tui.ui.screens.repo_picker import RepoPickerScreen

if TYPE_CHECKING:
    from types import SimpleNamespace

    from tests.tui.snapshot.conftest import MockAgentFactory
    from textual.pilot import Pilot

pytestmark = pytest.mark.usefixtures("global_mock_tmux")


class TestRepoPickerScreen:
    def test_repo_picker_with_multiple_repos(
        self,
        snapshot_project: SimpleNamespace,
        mock_acp_agent_factory: MockAgentFactory,
        snap_compare: Any,
        snapshot_terminal_size: tuple[int, int],
    ) -> None:
        """RepoPickerScreen shows a list of repos for selection."""
        from kagan.core.adapters.db.repositories import RepoRepository, TaskRepository
        from kagan.tui.app import KaganApp

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
                await repo_repo.add_to_project(
                    project_id,
                    repo.id,
                    is_primary=False,
                    display_order=1,
                )
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
