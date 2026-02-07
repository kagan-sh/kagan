"""Tests for auto-initializing git repos when adding empty folders.

Tests the critical user-facing behavior:
- NewProjectModal: empty folder → auto git init → project created
- NewProjectModal: valid git repo → direct create (no init)
- NewProjectModal: git init failure → no project created
- RepoPickerScreen: same behavior via action_add_repo
"""

from __future__ import annotations

from datetime import datetime
from types import SimpleNamespace
from typing import Any, cast
from unittest.mock import AsyncMock, patch

import pytest

from kagan.adapters.db.schema import Project
from kagan.git_utils import GitError, GitInitResult

# -- Helpers ------------------------------------------------------------------


def _fake_project() -> Project:
    """Create a minimal Project instance for tests."""
    return Project(
        id="proj-1",
        name="Test",
        description="",
        last_opened_at=None,
        created_at=datetime.now(),
        updated_at=datetime.now(),
    )


def _make_new_project_app(mock_project_service: Any):
    """TestApp for NewProjectModal tests."""
    from textual.app import App, ComposeResult
    from textual.widgets import Label

    class NewProjectTestApp(App):
        modal_result = None

        def compose(self) -> ComposeResult:
            yield Label("test")

        @property
        def ctx(self):
            return SimpleNamespace(project_service=mock_project_service)

    return NewProjectTestApp()


def _make_repo_picker_app(mock_project_service: Any):
    """TestApp with _ctx for RepoPickerScreen tests."""
    from textual.app import App, ComposeResult
    from textual.widgets import Label

    class RepoPickerTestApp(App):
        _ctx = SimpleNamespace(
            project_service=mock_project_service,
            task_service=AsyncMock(),
        )

        def compose(self) -> ComposeResult:
            yield Label("test")

    return RepoPickerTestApp()


# -- NewProjectModal ----------------------------------------------------------


class TestNewProjectModalGitInit:
    """Empty folder auto-inits git before project creation."""

    @pytest.mark.asyncio
    async def test_empty_folder_auto_init_creates_project(self, tmp_path):
        """Empty folder is auto-initialized with git before creating project."""
        from kagan.ui.modals.new_project import NewProjectModal

        empty_folder = tmp_path / "empty-project"
        empty_folder.mkdir()

        mock_svc = AsyncMock()
        mock_svc.create_project = AsyncMock(return_value="proj-123")
        app = _make_new_project_app(mock_svc)

        async with app.run_test() as pilot:
            from textual.widgets import Input

            modal = NewProjectModal()
            app.push_screen(modal, callback=lambda r: setattr(app, "modal_result", r))
            await pilot.pause()

            modal.query_one("#name-input", Input).value = "Test Project"
            modal.query_one("#path-input", Input).value = str(empty_folder)

            with patch(
                "kagan.git_utils.init_git_repo",
                new_callable=AsyncMock,
                return_value=GitInitResult(success=True, committed=True),
            ) as mock_init:
                await pilot.click("#btn-create")
                await pilot.pause()

                mock_init.assert_called_once_with(empty_folder, base_branch="main")

            mock_svc.create_project.assert_called_once()
            assert mock_svc.create_project.call_args.kwargs["name"] == "Test Project"

    @pytest.mark.asyncio
    async def test_empty_folder_does_not_open_git_confirm_modal(self, tmp_path):
        """Flow should never open an interactive git-init confirmation modal."""
        from kagan.ui.modals.new_project import NewProjectModal

        empty_folder = tmp_path / "empty-project"
        empty_folder.mkdir()

        mock_svc = AsyncMock()
        mock_svc.create_project = AsyncMock(return_value="proj-123")
        app = _make_new_project_app(mock_svc)

        async with app.run_test() as pilot:
            from textual.widgets import Input

            modal = NewProjectModal()
            app.push_screen(modal, callback=lambda r: setattr(app, "modal_result", r))
            await pilot.pause()

            modal.query_one("#name-input", Input).value = "Test Project"
            modal.query_one("#path-input", Input).value = str(empty_folder)

            app.push_screen_wait = AsyncMock()

            await pilot.click("#btn-create")
            await pilot.pause()

            app.push_screen_wait.assert_not_called()

    @pytest.mark.asyncio
    async def test_git_repo_path_skips_init(self, tmp_path):
        """Valid git repo → project created directly, no init call."""
        from tests.helpers.git import init_git_repo_with_commit

        from kagan.ui.modals.new_project import NewProjectModal

        repo = tmp_path / "valid-repo"
        repo.mkdir()
        await init_git_repo_with_commit(repo)

        mock_svc = AsyncMock()
        mock_svc.create_project = AsyncMock(return_value="proj-123")
        app = _make_new_project_app(mock_svc)

        async with app.run_test() as pilot:
            from textual.widgets import Input

            modal = NewProjectModal()
            app.push_screen(modal, callback=lambda r: setattr(app, "modal_result", r))
            await pilot.pause()

            modal.query_one("#name-input", Input).value = "Test Project"
            modal.query_one("#path-input", Input).value = str(repo)

            with patch("kagan.git_utils.init_git_repo", new_callable=AsyncMock) as mock_init:
                await pilot.click("#btn-create")
                await pilot.pause()

                mock_init.assert_not_called()

            mock_svc.create_project.assert_called_once()

    @pytest.mark.asyncio
    async def test_init_failure_blocks_project_creation(self, tmp_path):
        """Git init fails (e.g. no git user configured) → project NOT created."""
        from kagan.ui.modals.new_project import NewProjectModal

        empty_folder = tmp_path / "fail-project"
        empty_folder.mkdir()

        mock_svc = AsyncMock()
        mock_svc.create_project = AsyncMock(return_value="proj-123")
        app = _make_new_project_app(mock_svc)

        async with app.run_test() as pilot:
            from textual.widgets import Input

            modal = NewProjectModal()
            app.push_screen(modal, callback=lambda r: setattr(app, "modal_result", r))
            await pilot.pause()

            modal.query_one("#name-input", Input).value = "Test Project"
            modal.query_one("#path-input", Input).value = str(empty_folder)

            with patch(
                "kagan.git_utils.init_git_repo",
                new_callable=AsyncMock,
                return_value=GitInitResult(
                    success=False,
                    error=GitError(
                        error_type="user_not_configured",
                        message="Git user not configured",
                        details="Run: git config --global user.name",
                    ),
                ),
            ):
                await pilot.click("#btn-create")
                await pilot.pause()

            mock_svc.create_project.assert_not_called()


# -- RepoPickerScreen --------------------------------------------------------


class TestRepoPickerGitInit:
    """Auto-init scenarios exercised via the repo picker's action_add_repo."""

    @pytest.mark.asyncio
    async def test_empty_folder_auto_init_adds_repo(self, tmp_path):
        """Empty folder is auto-initialized with git before adding repo."""
        from kagan.ui.screens.repo_picker import RepoPickerScreen

        empty_folder = tmp_path / "empty-repo"
        empty_folder.mkdir()

        mock_svc = AsyncMock()
        mock_svc.add_repo_to_project = AsyncMock(return_value="repo-123")
        mock_svc.get_project_repos = AsyncMock(return_value=[])
        project = _fake_project()

        app = _make_repo_picker_app(mock_svc)
        async with app.run_test() as pilot:
            screen = RepoPickerScreen(project=cast("Any", project), repositories=[])
            await app.push_screen(screen)
            await pilot.pause()

            async def mock_psw(screen, **kw):
                from kagan.ui.modals.folder_picker import FolderPickerModal

                if isinstance(screen, FolderPickerModal):
                    return str(empty_folder)
                return None

            app.push_screen_wait = mock_psw  # type: ignore[assignment]

            with patch(
                "kagan.git_utils.init_git_repo",
                new_callable=AsyncMock,
                return_value=GitInitResult(success=True, committed=True),
            ):
                await screen.action_add_repo()
                await pilot.pause()

            mock_svc.add_repo_to_project.assert_called_once()

    @pytest.mark.asyncio
    async def test_empty_folder_flow_does_not_open_git_confirm_modal(self, tmp_path):
        """Flow should never open a git-init confirmation modal."""
        from kagan.ui.screens.repo_picker import RepoPickerScreen

        empty_folder = tmp_path / "empty-repo"
        empty_folder.mkdir()

        mock_svc = AsyncMock()
        mock_svc.add_repo_to_project = AsyncMock(return_value="repo-123")
        mock_svc.get_project_repos = AsyncMock(return_value=[])
        project = _fake_project()

        app = _make_repo_picker_app(mock_svc)
        async with app.run_test() as pilot:
            screen = RepoPickerScreen(project=cast("Any", project), repositories=[])
            await app.push_screen(screen)
            await pilot.pause()

            async def mock_psw(screen, **kw):
                from kagan.ui.modals.folder_picker import FolderPickerModal

                if isinstance(screen, FolderPickerModal):
                    return str(empty_folder)
                return None

            app.push_screen_wait = mock_psw  # type: ignore[assignment]

            with patch(
                "kagan.git_utils.init_git_repo",
                new_callable=AsyncMock,
                return_value=GitInitResult(success=True, committed=True),
            ):
                await screen.action_add_repo()
                await pilot.pause()

            mock_svc.add_repo_to_project.assert_called_once()

    @pytest.mark.asyncio
    async def test_git_repo_skips_init(self, tmp_path):
        """Valid git repo → added directly, no init call."""
        from tests.helpers.git import init_git_repo_with_commit

        from kagan.ui.screens.repo_picker import RepoPickerScreen

        repo = tmp_path / "valid-repo"
        repo.mkdir()
        await init_git_repo_with_commit(repo)

        mock_svc = AsyncMock()
        mock_svc.add_repo_to_project = AsyncMock(return_value="repo-123")
        mock_svc.get_project_repos = AsyncMock(return_value=[])
        project = _fake_project()

        app = _make_repo_picker_app(mock_svc)
        async with app.run_test() as pilot:
            screen = RepoPickerScreen(project=cast("Any", project), repositories=[])
            await app.push_screen(screen)
            await pilot.pause()

            async def mock_psw(screen, **kw):
                from kagan.ui.modals.folder_picker import FolderPickerModal

                if isinstance(screen, FolderPickerModal):
                    return str(repo)
                return None

            app.push_screen_wait = mock_psw  # type: ignore[assignment]

            with patch("kagan.git_utils.init_git_repo", new_callable=AsyncMock) as mock_init:
                await screen.action_add_repo()
                await pilot.pause()

                mock_init.assert_not_called()

            mock_svc.add_repo_to_project.assert_called_once()

    @pytest.mark.asyncio
    async def test_init_failure_does_not_add_repo(self, tmp_path):
        """Git init fails → repo NOT added."""
        from kagan.ui.screens.repo_picker import RepoPickerScreen

        empty_folder = tmp_path / "fail-repo"
        empty_folder.mkdir()

        mock_svc = AsyncMock()
        mock_svc.add_repo_to_project = AsyncMock(return_value="repo-123")
        mock_svc.get_project_repos = AsyncMock(return_value=[])
        project = _fake_project()

        app = _make_repo_picker_app(mock_svc)
        async with app.run_test() as pilot:
            screen = RepoPickerScreen(project=cast("Any", project), repositories=[])
            await app.push_screen(screen)
            await pilot.pause()

            async def mock_psw(screen, **kw):
                from kagan.ui.modals.folder_picker import FolderPickerModal

                if isinstance(screen, FolderPickerModal):
                    return str(empty_folder)
                return None

            app.push_screen_wait = mock_psw  # type: ignore[assignment]

            with patch(
                "kagan.git_utils.init_git_repo",
                new_callable=AsyncMock,
                return_value=GitInitResult(
                    success=False,
                    error=GitError(error_type="init_failed", message="Failed"),
                ),
            ):
                await screen.action_add_repo()
                await pilot.pause()

            mock_svc.add_repo_to_project.assert_not_called()
