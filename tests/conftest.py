"""Pytest fixtures for Kagan tests."""

from __future__ import annotations

import asyncio
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
    from kagan.services.tasks import TaskServiceImpl

# =============================================================================
# Hypothesis Profiles
# =============================================================================

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


# =============================================================================
# Core Unit Test Fixtures
# =============================================================================


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
        yield manager
        await manager.close()


@pytest.fixture
def event_bus() -> InMemoryEventBus:
    """Create an in-memory event bus for service tests."""
    from kagan.bootstrap import InMemoryEventBus

    return InMemoryEventBus()


@pytest.fixture
def task_service(state_manager: TaskRepository, event_bus: InMemoryEventBus) -> TaskServiceImpl:
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
        session_active: bool = False,
        total_iterations: int = 0,
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
            session_active=session_active,
            total_iterations=total_iterations,
            project_id=project_id,
        )

    return _factory


@pytest.fixture
def app() -> KaganApp:
    """Create app with in-memory database."""
    from kagan.app import KaganApp

    return KaganApp(db_path=":memory:")


# =============================================================================
# Git Repository Fixtures
# =============================================================================


@pytest.fixture
async def git_repo(tmp_path: Path) -> Path:
    """Create an initialized git repository for testing.

    Shared by: test_worktree.py, test_git_utils.py, and other git tests.

    Provides:
    - Initialized git repo with 'main' branch
    - Configured user (email, name)
    - GPG signing disabled
    - Initial commit with README.md
    """
    from tests.helpers.git import init_git_repo_with_commit

    return await init_git_repo_with_commit(tmp_path)


# =============================================================================
# Mock Fixtures
# =============================================================================


@pytest.fixture
def mock_agent():
    """Create a mock ACP agent for testing.

    Shared by: test_scheduler.py and other agent tests.
    Default response: "Done! <complete/>"
    """
    from tests.helpers.mocks import create_mock_agent

    return create_mock_agent()


@pytest.fixture
def mock_workspace_service():
    """Create a mock WorkspaceService."""
    from tests.helpers.mocks import create_mock_workspace_service

    return create_mock_workspace_service()


@pytest.fixture
def config():
    """Create a test KaganConfig."""
    from tests.helpers.mocks import create_test_config

    return create_test_config()


@pytest.fixture
def agent_config():
    """Create a minimal AgentConfig for testing."""
    from tests.helpers.mocks import create_test_agent_config

    return create_test_agent_config()


@pytest.fixture
def mock_process():
    """Create a mock subprocess for agent process testing."""
    from tests.helpers.mocks import create_mock_process

    return create_mock_process()


# =============================================================================
# E2E Test Fixtures
# =============================================================================


async def _create_e2e_app_with_tasks(e2e_project, tasks: list[dict]) -> KaganApp:
    """Helper to create a KaganApp with pre-populated tasks."""
    from kagan.adapters.db.repositories import TaskRepository
    from kagan.adapters.db.schema import Task
    from kagan.app import KaganApp

    manager = TaskRepository(e2e_project.db)
    await manager.initialize()
    project_id = manager.default_project_id
    if project_id is None:
        raise RuntimeError("TaskRepository defaults not initialized")
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
        lock_path=None,
        project_root=e2e_project.root,
    )


@pytest.fixture
async def e2e_project(tmp_path: Path):
    """Create a real project with git repo and kagan config for E2E testing.

    This fixture provides:
    - A real git repository with initial commit
    - A config.toml file stored outside the repo
    - Paths to DB and config for KaganApp initialization
    """
    project = tmp_path / "test_project"
    project.mkdir()

    # Initialize real git repo with commit
    from tests.helpers.git import init_git_repo_with_commit

    await init_git_repo_with_commit(project)

    config_dir = tmp_path / "kagan-config"
    config_dir.mkdir()
    data_dir = tmp_path / "kagan-data"
    data_dir.mkdir()

    config_content = """# Kagan Test Configuration
[general]
auto_start = false
auto_merge = false
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
    """Mock ACP agent subprocess spawning.

    This is the ONLY mock we use in E2E tests - everything else is real.
    The mock prevents actual agent CLI processes from starting.
    """
    original_exec = asyncio.create_subprocess_exec

    async def selective_mock(*args, **kwargs):
        # Only mock agent-related commands, allow git commands through
        cmd = args[0] if args else ""
        if cmd in ("git", "tmux"):
            return await original_exec(*args, **kwargs)

        # Mock agent processes
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
    """Factory that returns a mock Agent for testing.

    Usage in tests:
        async def test_something(state_manager, mock_agent_factory):
            automation = AutomationServiceImpl(
                task_service=task_service,
                workspace_service=worktrees,
                config=config,
                agent_factory=mock_agent_factory,
            )
    """
    from kagan.acp.agent import Agent
    from kagan.acp.buffers import AgentBuffers

    def factory(project_root, agent_config, *, read_only=False):
        mock_agent = MagicMock(spec=Agent)
        buffers = AgentBuffers()
        buffers.append_response("Done. <complete/>")

        mock_agent.set_auto_approve = MagicMock()
        mock_agent.set_model_override = MagicMock()
        mock_agent.start = MagicMock()
        mock_agent.wait_ready = AsyncMock()
        mock_agent.send_prompt = AsyncMock()
        mock_agent.get_response_text = MagicMock(side_effect=buffers.get_response_text)
        mock_agent.get_tool_calls = MagicMock(return_value=[])
        mock_agent.get_thinking_text = MagicMock(return_value="")
        mock_agent.clear_tool_calls = MagicMock()
        mock_agent.stop = AsyncMock()
        mock_agent._buffers = buffers
        return mock_agent

    return factory


@pytest.fixture
async def e2e_app(e2e_project):
    """Create a KaganApp configured for E2E testing with real git repo."""
    from kagan.app import KaganApp

    app = KaganApp(
        db_path=e2e_project.db,
        config_path=e2e_project.config,
        lock_path=None,
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


@pytest.fixture
async def e2e_app_with_auto_task(e2e_project):
    """Create a KaganApp with an AUTO task in IN_PROGRESS."""
    return await _create_e2e_app_with_tasks(
        e2e_project,
        [
            dict(
                title="Auto task in progress",
                description="An AUTO task currently being worked on by agent",
                priority=TaskPriority.HIGH,
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.AUTO,
            )
        ],
    )


@pytest.fixture
async def e2e_app_with_done_task(e2e_project):
    """Create a KaganApp with a task in DONE status."""
    return await _create_e2e_app_with_tasks(
        e2e_project,
        [
            dict(
                title="Completed task",
                description="A task that has been completed",
                priority=TaskPriority.MEDIUM,
                status=TaskStatus.DONE,
            )
        ],
    )


@pytest.fixture
async def e2e_app_with_ac_task(e2e_project):
    """Create a KaganApp with a task that has acceptance criteria."""
    return await _create_e2e_app_with_tasks(
        e2e_project,
        [
            dict(
                title="Task with acceptance criteria",
                description="A task with defined acceptance criteria",
                priority=TaskPriority.HIGH,
                status=TaskStatus.BACKLOG,
                acceptance_criteria=["User can login", "Error messages shown"],
            )
        ],
    )


def _create_fake_tmux(sessions: dict):
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
                # Extract environment variables from -e flags
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
            name, keys = args_list[idx + 1], args_list[idx + 2] if idx + 2 < len(args_list) else ""
            if name in sessions:
                sessions[name]["sent_keys"].append(keys)
        elif command == "list-sessions":
            return "\n".join(sorted(sessions.keys()))
        return ""

    return fake_run_tmux


@pytest.fixture(autouse=True)
def auto_mock_tmux_for_app_tests(request, monkeypatch):
    """Auto-mock tmux for tests using KaganApp fixtures (external system boundary)."""
    # Match fixtures that create KaganApp instances
    app_fixture_patterns = ("e2e_app", "app", "welcome_app", "_fresh_app")
    if not any(
        n.startswith(app_fixture_patterns) or n in app_fixture_patterns
        for n in request.fixturenames
    ):
        return
    fake = _create_fake_tmux({})
    monkeypatch.setattr("kagan.tmux.run_tmux", fake)
    monkeypatch.setattr("kagan.services.sessions.run_tmux", fake)


@pytest.fixture
def mock_tmux(monkeypatch):
    """Intercept tmux calls and return session state for assertions."""
    sessions: dict = {}
    monkeypatch.setattr("kagan.services.sessions.run_tmux", _create_fake_tmux(sessions))
    return sessions
