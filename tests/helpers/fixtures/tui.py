"""TUI and E2E test fixtures: app creation, e2e projects, terminal mocking."""

from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock

import pytest

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.tui.app import KaganApp


@pytest.fixture
def app() -> KaganApp:
    """Create app with in-memory database."""
    from kagan.tui.app import KaganApp

    return KaganApp(db_path=":memory:")


@pytest.fixture
async def e2e_project(tmp_path: Path):
    """Create a real project with git repo and kagan config for E2E testing."""
    project = tmp_path / "test_project"
    project.mkdir()

    from tests.helpers.git import init_git_repo_with_commit

    await init_git_repo_with_commit(project)

    config_dir = tmp_path / "kagan-config"
    config_dir.mkdir()
    data_dir = tmp_path / "kagan-data"
    data_dir.mkdir()

    config_content = """# Kagan Test Configuration
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
    config_path = config_dir / "config.toml"
    config_path.write_text(config_content)

    return SimpleNamespace(
        root=project,
        db=str(data_dir / "kagan.db"),
        config=str(config_path),
    )


async def _create_e2e_app_with_tasks(e2e_project, tasks: list[dict]) -> KaganApp:
    """Helper to create a KaganApp with pre-populated tasks."""
    from kagan.core.adapters.db.repositories import RepoRepository, TaskRepository
    from kagan.core.adapters.db.schema import Task
    from kagan.tui.app import KaganApp

    manager = TaskRepository(e2e_project.db, project_root=e2e_project.root)
    await manager.initialize()

    project_id = await manager.ensure_test_project("E2E Test Project")

    assert manager._session_factory is not None
    repo_repo = RepoRepository(manager._session_factory)
    repo, _ = await repo_repo.get_or_create(e2e_project.root, default_branch="main")
    if repo.id:
        await repo_repo.update_default_branch(repo.id, "main", mark_configured=True)
        await repo_repo.add_to_project(project_id, repo.id, is_primary=True)

    for task_kwargs in tasks:
        task = Task.create(
            project_id=project_id,
            **task_kwargs,
        )
        await manager.create(task)
    await manager.close()
    return KaganApp(
        db_path=e2e_project.db,
        config_path=e2e_project.config,
        project_root=e2e_project.root,
    )


@pytest.fixture
async def e2e_app(e2e_project):
    """Create a KaganApp configured for E2E testing with real git repo."""
    from kagan.tui.app import KaganApp

    app = KaganApp(
        db_path=e2e_project.db,
        config_path=e2e_project.config,
        project_root=e2e_project.root,
    )
    return app


@pytest.fixture
async def e2e_app_with_tasks(e2e_project):
    """Create a KaganApp with pre-populated tasks (backlog, in-progress, review)."""
    from kagan.core.models.enums import TaskPriority, TaskStatus

    return await _create_e2e_app_with_tasks(
        e2e_project,
        [
            dict(
                title="Backlog task",
                description="A task in backlog",
                priority=TaskPriority.LOW,
                status=TaskStatus.BACKLOG,
            ),
            dict(
                title="In progress task",
                description="Currently working",
                priority=TaskPriority.HIGH,
                status=TaskStatus.IN_PROGRESS,
            ),
            dict(
                title="Review task",
                description="Ready for review",
                priority=TaskPriority.MEDIUM,
                status=TaskStatus.REVIEW,
            ),
        ],
    )


@pytest.fixture(autouse=True)
def auto_mock_terminals_for_app_tests(request, monkeypatch):
    """Auto-mock terminal backends for app-driven tests (external system boundary)."""
    from tests.helpers.mocks import install_fake_tmux

    app_fixture_patterns = ("e2e_app", "app", "welcome_app", "_fresh_app")
    if not any(
        n.startswith(app_fixture_patterns) or n in app_fixture_patterns
        for n in request.fixturenames
    ):
        return

    install_fake_tmux(monkeypatch)

    # Safety: never spawn real terminal attach/external launcher subprocesses in app tests.
    monkeypatch.setattr(
        "kagan.core.services.sessions.SessionServiceImpl._attach_tmux_session",
        AsyncMock(return_value=True),
    )
    monkeypatch.setattr(
        "kagan.core.services.sessions.SessionServiceImpl._launch_external_launcher",
        AsyncMock(return_value=True),
    )

    # Assume terminals are available in app tests unless a test overrides this explicitly.
    monkeypatch.setattr(
        "kagan.tui.terminals.installer.check_terminal_installed", lambda _name: True
    )


@pytest.fixture
def global_mock_tmux(monkeypatch: pytest.MonkeyPatch) -> dict[str, dict[str, object]]:
    """Install fake tmux handlers for tests via a shared fixture."""
    from tests.helpers.mocks import install_fake_tmux

    return install_fake_tmux(monkeypatch)


@pytest.fixture
def mock_tmux(
    global_mock_tmux: dict[str, dict[str, object]],
) -> dict[str, dict[str, object]]:
    """Intercept tmux calls and return session state for assertions."""
    return global_mock_tmux
