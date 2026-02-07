"""Planner uses active repo context when switching repositories."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, cast

import pytest
from tests.helpers.git import init_git_repo_with_commit
from tests.helpers.mocks import MockAgent
from tests.helpers.wait import (
    type_text,
    wait_for_planner_ready,
    wait_for_screen,
)

from kagan.acp import messages
from kagan.app import KaganApp
from kagan.ui.screens.planner import PlannerInput, PlannerScreen
from kagan.ui.screens.repo_picker import RepoPickerScreen
from kagan.ui.widgets import StatusBar, StreamingOutput

if TYPE_CHECKING:
    from pathlib import Path


class RepoEchoAgent(MockAgent):
    """Mock agent that echoes its working folder in the response."""

    def __init__(self, project_root: Path, agent_config: Any, *, read_only: bool = False) -> None:
        super().__init__(project_root, agent_config, read_only=read_only)
        self.set_response(f"I'm currently working in: {project_root}")


class RepoEchoAgentFactory:
    """Factory that returns RepoEchoAgent instances."""

    def __call__(self, project_root: Path, agent_config: Any, *, read_only: bool = False) -> Any:
        return RepoEchoAgent(project_root, agent_config, read_only=read_only)


def _mock_tmux(monkeypatch: pytest.MonkeyPatch) -> None:
    async def fake_run_tmux(*_args: str) -> str:
        return ""

    monkeypatch.setattr("kagan.tmux.run_tmux", fake_run_tmux)
    monkeypatch.setattr("kagan.services.sessions.run_tmux", fake_run_tmux)


def _read_planner_output(screen: PlannerScreen) -> str:
    from kagan.ui.widgets.streaming_markdown import StreamingMarkdown

    return "\n".join(widget.content for widget in screen.query(StreamingMarkdown))


@pytest.mark.asyncio
async def test_planner_uses_active_repo_context(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo_a = tmp_path / "repo-a"
    repo_b = tmp_path / "repo-b"
    repo_a.mkdir()
    repo_b.mkdir()
    await init_git_repo_with_commit(repo_a)
    await init_git_repo_with_commit(repo_b)

    config_path = tmp_path / "config.toml"
    config_path.write_text(
        """# Test config
[general]
auto_review = false
default_base_branch = "main"
default_worker_agent = "claude"

[agents.claude]
identity = "claude.ai"
name = "Claude"
short_name = "claude"
run_command."*" = "echo mock-claude"
interactive_command."*" = "echo mock-claude-interactive"
active = true
"""
    )

    db_path = tmp_path / "kagan.db"
    from kagan.adapters.db.repositories import RepoRepository, TaskRepository

    task_repo = TaskRepository(db_path, project_root=repo_a)
    await task_repo.initialize()
    project_id = await task_repo.ensure_test_project("Planner Repo Context")
    assert task_repo._session_factory is not None
    repo_repo = RepoRepository(task_repo._session_factory)
    repo_a_row, _ = await repo_repo.get_or_create(repo_a, default_branch="main")
    repo_b_row, _ = await repo_repo.get_or_create(repo_b, default_branch="main")
    if repo_a_row.id:
        await repo_repo.add_to_project(project_id, repo_a_row.id, is_primary=True)
    if repo_b_row.id:
        await repo_repo.add_to_project(project_id, repo_b_row.id, is_primary=False)
    await task_repo.close()

    _mock_tmux(monkeypatch)

    app = KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=repo_a,
        agent_factory=RepoEchoAgentFactory(),
    )

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
        await wait_for_planner_ready(pilot, timeout=10.0)

        screen = cast("PlannerScreen", pilot.app.screen)
        screen.query_one(PlannerInput).focus()
        await type_text(pilot, "where")
        await pilot.press("enter")
        await pilot.pause()
        await pilot.pause()
        assert str(repo_a) in _read_planner_output(screen)

        await pilot.press("ctrl+r")
        await wait_for_screen(pilot, RepoPickerScreen)
        await pilot.press("down")
        await pilot.press("enter")
        await wait_for_screen(pilot, PlannerScreen)
        await wait_for_planner_ready(pilot, timeout=10.0)

        screen = cast("PlannerScreen", pilot.app.screen)
        screen.query_one(PlannerInput).focus()
        await type_text(pilot, "where")
        await pilot.press("enter")
        await pilot.pause()
        await pilot.pause()
        assert str(repo_b) in _read_planner_output(screen)


@pytest.mark.asyncio
async def test_planner_sigterm_stream_end_is_not_rendered_as_error(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    repo = tmp_path / "repo"
    repo.mkdir()
    await init_git_repo_with_commit(repo)

    config_path = tmp_path / "config.toml"
    config_path.write_text(
        """# Test config
[general]
auto_review = false
default_base_branch = "main"
default_worker_agent = "claude"

[agents.claude]
identity = "claude.ai"
name = "Claude"
short_name = "claude"
run_command."*" = "echo mock-claude"
interactive_command."*" = "echo mock-claude-interactive"
active = true
"""
    )

    db_path = tmp_path / "kagan.db"
    from kagan.adapters.db.repositories import RepoRepository, TaskRepository

    task_repo = TaskRepository(db_path, project_root=repo)
    await task_repo.initialize()
    project_id = await task_repo.ensure_test_project("Planner SIGTERM")
    assert task_repo._session_factory is not None
    repo_repo = RepoRepository(task_repo._session_factory)
    repo_row, _ = await repo_repo.get_or_create(repo, default_branch="main")
    if repo_row.id:
        await repo_repo.add_to_project(project_id, repo_row.id, is_primary=True)
    await task_repo.close()

    _mock_tmux(monkeypatch)

    app = KaganApp(
        db_path=str(db_path),
        config_path=str(config_path),
        project_root=repo,
        agent_factory=RepoEchoAgentFactory(),
    )

    async with app.run_test(size=(120, 40)) as pilot:
        await wait_for_screen(pilot, PlannerScreen, timeout=10.0)
        await wait_for_planner_ready(pilot, timeout=10.0)

        screen = cast("PlannerScreen", pilot.app.screen)
        screen.post_message(messages.AgentFail("Agent exited with code -15"))
        await pilot.pause()
        await pilot.pause()

        rendered = screen.query_one("#planner-output", StreamingOutput).get_text_content()
        status = screen.query_one("#planner-status-bar", StatusBar)
        planner_input = screen.query_one(PlannerInput)
        assert "Agent stream ended by cancellation (SIGTERM)." in rendered
        assert "Error: Agent exited with code -15" not in rendered
        assert status.status == "ready"
        assert planner_input.disabled is False
