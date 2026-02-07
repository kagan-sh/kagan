"""Snapshot tests for RepoPickerScreen."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Any, cast

import pytest

from kagan.ui.screens.repo_picker import RepoPickerScreen
from tests.helpers.git import init_git_repo_with_commit

if TYPE_CHECKING:
    from types import SimpleNamespace

    from textual.pilot import Pilot

    from tests.snapshots.conftest import MockAgentFactory


def _create_fake_tmux(sessions: dict[str, Any]) -> object:
    """Create a fake tmux function that tracks session state."""

    async def fake_run_tmux(*args: str) -> str:
        if not args:
            return ""
        command, args_list = args[0], list(args)
        if command == "new-session" and "-s" in args_list:
            idx = args_list.index("-s")
            name = args_list[idx + 1] if idx + 1 < len(args_list) else None
            if name:
                cwd = args_list[args_list.index("-c") + 1] if "-c" in args_list else ""
                env: dict[str, str] = {}
                for i, val in enumerate(args_list):
                    if val == "-e" and i + 1 < len(args_list):
                        key, _, env_value = args_list[i + 1].partition("=")
                        env[key] = env_value
                sessions[name] = {"cwd": cwd, "env": env, "sent_keys": []}
        elif command == "kill-session" and "-t" in args_list:
            sessions.pop(args_list[args_list.index("-t") + 1], None)
        elif command == "send-keys" and "-t" in args_list:
            idx = args_list.index("-t")
            name = args_list[idx + 1]
            keys = args_list[idx + 2] if idx + 2 < len(args_list) else ""
            if name in sessions:
                sessions[name]["sent_keys"].append(keys)
        elif command == "list-sessions":
            return "\n".join(sorted(sessions.keys()))
        return ""

    return fake_run_tmux


class TestRepoPickerScreen:
    """Snapshot tests for RepoPickerScreen."""

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

        # Mock tmux
        sessions: dict[str, Any] = {}
        fake_tmux = _create_fake_tmux(sessions)
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

            # Create an additional repo for the project
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
