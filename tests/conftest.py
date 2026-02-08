"""Pytest fixtures for Kagan tests."""

from __future__ import annotations

import asyncio
import json
import os
import shutil
import tempfile
from pathlib import Path
from types import SimpleNamespace
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock

import pytest
from hypothesis import Phase, Verbosity, settings

from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType

_TEST_BASE_DIR = Path(tempfile.mkdtemp(prefix="kagan-tests-"))
os.environ["KAGAN_DATA_DIR"] = str(_TEST_BASE_DIR / "data")
os.environ["KAGAN_CONFIG_DIR"] = str(_TEST_BASE_DIR / "config")
os.environ["KAGAN_CACHE_DIR"] = str(_TEST_BASE_DIR / "cache")
os.environ["KAGAN_WORKTREE_BASE"] = str(_TEST_BASE_DIR / "worktrees")

if TYPE_CHECKING:
    from collections.abc import Generator

    from kagan.adapters.db.repositories import TaskRepository
    from kagan.adapters.db.schema import Task
    from kagan.app import KaganApp
    from kagan.bootstrap import InMemoryEventBus
    from kagan.services.tasks import TaskService


settings.register_profile(
    "ci",
    max_examples=100,
    deadline=None,
    phases=[Phase.explicit, Phase.reuse, Phase.generate, Phase.shrink],
)
settings.register_profile(
    "dev",
    max_examples=20,
    deadline=500,
)
settings.register_profile(
    "debug",
    max_examples=10,
    verbosity=Verbosity.verbose,
    deadline=None,
)
settings.load_profile(os.getenv("HYPOTHESIS_PROFILE", "dev"))


def pytest_collection_modifyitems(config: pytest.Config, items: list[pytest.Item]) -> None:
    """Ensure snapshot tests run sequentially under xdist."""
    del config
    for item in items:
        if item.get_closest_marker("snapshot"):
            item.add_marker(pytest.mark.xdist_group("snapshots"))


@pytest.fixture(autouse=True)
def _mock_platform_system(monkeypatch, request):
    """Handle @pytest.mark.mock_platform_system("Windows") marker."""
    marker = request.node.get_closest_marker("mock_platform_system")
    if marker:
        target_platform = marker.args[0]
        monkeypatch.setattr("platform.system", lambda: target_platform)
        # Also mock command_utils.is_windows which caches platform.system
        monkeypatch.setattr("kagan.command_utils.is_windows", lambda: target_platform == "Windows")


@pytest.fixture(autouse=True)
def _clean_worktree_base() -> Generator[None, None, None]:
    """Ensure worktree temp directories don't leak between tests."""
    yield
    base_dir = Path(os.environ["KAGAN_WORKTREE_BASE"])
    shutil.rmtree(base_dir, ignore_errors=True)


@pytest.fixture
async def state_manager():
    """Create a temporary task repository for testing."""
    from kagan.adapters.db.repositories import TaskRepository

    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = Path(tmpdir) / "test.db"
        manager = TaskRepository(db_path)
        await manager.initialize()

        await manager.ensure_test_project("Test Project")
        yield manager
        await manager.close()


@pytest.fixture
def event_bus() -> InMemoryEventBus:
    """Create an in-memory event bus for service tests."""
    from kagan.bootstrap import InMemoryEventBus

    return InMemoryEventBus()


@pytest.fixture
def task_service(state_manager: TaskRepository, event_bus: InMemoryEventBus) -> TaskService:
    """Create a TaskService backed by the test repository."""
    from kagan.services.tasks import TaskServiceImpl

    return TaskServiceImpl(state_manager, event_bus)


@pytest.fixture
def task_factory(state_manager: TaskRepository):
    """Factory for creating DB Task objects with default project/repo IDs."""
    from kagan.adapters.db.schema import Task
    from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType

    def _factory(
        *,
        title: str,
        description: str = "",
        priority: TaskPriority = TaskPriority.MEDIUM,
        status: TaskStatus = TaskStatus.BACKLOG,
        task_type: TaskType = TaskType.PAIR,
        acceptance_criteria: list[str] | None = None,
        assigned_hat: str | None = None,
        agent_backend: str | None = None,
    ) -> Task:
        project_id = state_manager.default_project_id
        if project_id is None:
            raise RuntimeError("TaskRepository defaults not initialized")
        return Task.create(
            title=title,
            description=description,
            priority=priority,
            task_type=task_type,
            status=status,
            assigned_hat=assigned_hat,
            agent_backend=agent_backend,
            acceptance_criteria=acceptance_criteria,
            project_id=project_id,
        )

    return _factory


@pytest.fixture
def app() -> KaganApp:
    """Create app with in-memory database."""
    from kagan.app import KaganApp

    return KaganApp(db_path=":memory:")


@pytest.fixture
async def git_repo(tmp_path: Path) -> Path:
    """Create an initialized git repository for testing."""
    from tests.helpers.git import init_git_repo_with_commit

    return await init_git_repo_with_commit(tmp_path)


@pytest.fixture
def mock_workspace_service():
    """Create a mock WorkspaceService."""
    from tests.helpers.mocks import create_mock_workspace_service

    return create_mock_workspace_service()


async def _create_e2e_app_with_tasks(e2e_project, tasks: list[dict]) -> KaganApp:
    """Helper to create a KaganApp with pre-populated tasks."""
    from kagan.adapters.db.repositories import RepoRepository, TaskRepository
    from kagan.adapters.db.schema import Task
    from kagan.app import KaganApp

    manager = TaskRepository(e2e_project.db, project_root=e2e_project.root)
    await manager.initialize()

    project_id = await manager.ensure_test_project("E2E Test Project")

    assert manager._session_factory is not None
    repo_repo = RepoRepository(manager._session_factory)
    repo, _ = await repo_repo.get_or_create(e2e_project.root, default_branch="main")
    if repo.id:
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


@pytest.fixture
def mock_agent_spawn(monkeypatch):
    """Mock ACP agent subprocess spawning."""
    original_exec = asyncio.create_subprocess_exec

    async def selective_mock(*args, **kwargs):
        cmd = args[0] if args else ""
        if cmd in ("git", "tmux"):
            return await original_exec(*args, **kwargs)

        mock_process = MagicMock()
        mock_process.pid = 12345
        mock_process.returncode = None
        mock_process.stdout = MagicMock()
        mock_process.stdout.readline = AsyncMock(return_value=b"")
        mock_process.stderr = MagicMock()
        mock_process.stderr.readline = AsyncMock(return_value=b"")
        mock_process.wait = AsyncMock(return_value=0)
        mock_process.terminate = MagicMock()
        mock_process.kill = MagicMock()
        mock_process.communicate = AsyncMock(return_value=(b"", b""))
        return mock_process

    monkeypatch.setattr("asyncio.create_subprocess_exec", selective_mock)


@pytest.fixture
def mock_agent_factory():
    """Factory that returns deterministic mock agents for testing."""
    from tests.helpers.mocks import MockAgentFactory

    return MockAgentFactory()


@pytest.fixture
async def e2e_app(e2e_project):
    """Create a KaganApp configured for E2E testing with real git repo."""
    from kagan.app import KaganApp

    app = KaganApp(
        db_path=e2e_project.db,
        config_path=e2e_project.config,
        project_root=e2e_project.root,
    )
    return app


@pytest.fixture
async def e2e_app_with_tasks(e2e_project):
    """Create a KaganApp with pre-populated tasks (backlog, in-progress, review)."""
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
    from tests.helpers.mocks import create_fake_tmux

    app_fixture_patterns = ("e2e_app", "app", "welcome_app", "_fresh_app")
    if not any(
        n.startswith(app_fixture_patterns) or n in app_fixture_patterns
        for n in request.fixturenames
    ):
        return

    # tmux commands
    fake = create_fake_tmux({})
    monkeypatch.setattr("kagan.tmux.run_tmux", fake)
    monkeypatch.setattr("kagan.services.sessions.run_tmux", fake)

    # wezterm commands
    wezterm_workspaces: dict[str, set[str]] = {}
    pane_counter = 0

    async def fake_run_wezterm(*args: str, **_kwargs: object) -> str:
        nonlocal pane_counter
        if not args:
            return ""

        if args[0] == "start":
            workspace = "default"
            if "--workspace" in args:
                idx = args.index("--workspace")
                if idx + 1 < len(args):
                    workspace = args[idx + 1]
            pane_counter += 1
            wezterm_workspaces.setdefault(workspace, set()).add(str(pane_counter))
            return ""

        if args[:3] == ("cli", "list", "--format"):
            payload = [
                {"workspace": workspace, "pane_id": pane_id}
                for workspace, pane_ids in wezterm_workspaces.items()
                for pane_id in sorted(pane_ids)
            ]
            return json.dumps(payload)

        if args[:3] == ("cli", "kill-pane", "--pane-id") and len(args) >= 4:
            pane_id = args[3]
            for pane_ids in wezterm_workspaces.values():
                pane_ids.discard(pane_id)
            empty = [
                workspace for workspace, pane_ids in wezterm_workspaces.items() if not pane_ids
            ]
            for workspace in empty:
                wezterm_workspaces.pop(workspace, None)
            return ""

        return ""

    monkeypatch.setattr("kagan.wezterm.run_wezterm", fake_run_wezterm)
    monkeypatch.setattr("kagan.services.sessions.run_wezterm", fake_run_wezterm)

    # Safety: never spawn real terminal attach/external launcher subprocesses in app tests.
    monkeypatch.setattr(
        "kagan.services.sessions.SessionServiceImpl._attach_tmux_session",
        AsyncMock(return_value=True),
    )
    monkeypatch.setattr(
        "kagan.services.sessions.SessionServiceImpl._attach_wezterm_session",
        AsyncMock(return_value=True),
    )
    monkeypatch.setattr(
        "kagan.services.sessions.SessionServiceImpl._launch_external_launcher",
        AsyncMock(return_value=True),
    )

    # Assume terminals are available in app tests unless a test overrides this explicitly.
    monkeypatch.setattr("kagan.terminals.installer.check_terminal_installed", lambda _name: True)


@pytest.fixture
def mock_tmux(monkeypatch):
    """Intercept tmux calls and return session state for assertions."""
    from tests.helpers.mocks import create_fake_tmux

    sessions: dict = {}
    monkeypatch.setattr("kagan.services.sessions.run_tmux", create_fake_tmux(sessions))
    return sessions
