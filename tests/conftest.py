"""Pytest fixtures for Kagan tests."""

from __future__ import annotations

import asyncio
import os
import tempfile
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

import pytest
from hypothesis import Phase, Verbosity, settings

from kagan.app import KaganApp
from kagan.database.manager import StateManager
from kagan.database.models import Ticket, TicketPriority, TicketStatus, TicketType
from tests.helpers.git import init_git_repo_with_commit
from tests.helpers.mocks import (
    create_mock_agent,
    create_mock_process,
    create_mock_session_manager,
    create_mock_worktree_manager,
    create_test_agent_config,
    create_test_config,
)

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

# =============================================================================
# Core Unit Test Fixtures
# =============================================================================


@pytest.fixture
async def state_manager():
    """Create a temporary database for testing.

    Shared by: test_database.py, test_scheduler.py, and other DB tests.
    """
    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = Path(tmpdir) / "test.db"
        manager = StateManager(db_path)
        await manager.initialize()
        yield manager
        await manager.close()


@pytest.fixture
def app() -> KaganApp:
    """Create app with in-memory database."""
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
    return create_mock_agent()


@pytest.fixture
def mock_worktree_manager():
    """Create a mock WorktreeManager."""
    return create_mock_worktree_manager()


@pytest.fixture
def mock_session_manager():
    """Create a mock SessionManager."""
    return create_mock_session_manager()


@pytest.fixture
def config():
    """Create a test KaganConfig."""
    return create_test_config()


@pytest.fixture
def agent_config():
    """Create a minimal AgentConfig for testing."""
    return create_test_agent_config()


@pytest.fixture
def mock_process():
    """Create a mock subprocess for agent process testing."""
    return create_mock_process()


# =============================================================================
# E2E Test Fixtures
# =============================================================================


async def _create_e2e_app_with_tickets(e2e_project, tickets: list[Ticket]) -> KaganApp:
    """Helper to create a KaganApp with pre-populated tickets."""
    manager = StateManager(e2e_project.db)
    await manager.initialize()
    for ticket in tickets:
        await manager.create_ticket(ticket)
    await manager.close()
    return KaganApp(db_path=e2e_project.db, config_path=e2e_project.config, lock_path=None)


@pytest.fixture
async def e2e_project(tmp_path: Path):
    """Create a real project with git repo and kagan config for E2E testing.

    This fixture provides:
    - A real git repository with initial commit
    - A .kagan/config.toml file
    - Paths to DB and config for KaganApp initialization
    """
    project = tmp_path / "test_project"
    project.mkdir()

    # Initialize real git repo with commit
    await init_git_repo_with_commit(project)

    # Create .kagan directory with config
    kagan_dir = project / ".kagan"
    kagan_dir.mkdir()

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
    (kagan_dir / "config.toml").write_text(config_content)

    return SimpleNamespace(
        root=project,
        db=str(kagan_dir / "state.db"),
        config=str(kagan_dir / "config.toml"),
        kagan_dir=kagan_dir,
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
async def e2e_app(e2e_project):
    """Create a KaganApp configured for E2E testing with real git repo."""
    app = KaganApp(
        db_path=e2e_project.db,
        config_path=e2e_project.config,
        lock_path=None,
    )
    return app


@pytest.fixture
async def e2e_app_with_tickets(e2e_project):
    """Create a KaganApp with pre-populated tickets (backlog, in-progress, review)."""
    return await _create_e2e_app_with_tickets(
        e2e_project,
        [
            Ticket.create(
                title="Backlog task",
                description="A task in backlog",
                priority=TicketPriority.LOW,
                status=TicketStatus.BACKLOG,
            ),
            Ticket.create(
                title="In progress task",
                description="Currently working",
                priority=TicketPriority.HIGH,
                status=TicketStatus.IN_PROGRESS,
            ),
            Ticket.create(
                title="Review task",
                description="Ready for review",
                priority=TicketPriority.MEDIUM,
                status=TicketStatus.REVIEW,
            ),
        ],
    )


@pytest.fixture
async def e2e_app_with_auto_ticket(e2e_project):
    """Create a KaganApp with an AUTO ticket in IN_PROGRESS."""
    return await _create_e2e_app_with_tickets(
        e2e_project,
        [
            Ticket.create(
                title="Auto task in progress",
                description="An AUTO task currently being worked on by agent",
                priority=TicketPriority.HIGH,
                status=TicketStatus.IN_PROGRESS,
                ticket_type=TicketType.AUTO,
            )
        ],
    )


@pytest.fixture
async def e2e_app_with_done_ticket(e2e_project):
    """Create a KaganApp with a ticket in DONE status."""
    return await _create_e2e_app_with_tickets(
        e2e_project,
        [
            Ticket.create(
                title="Completed task",
                description="A task that has been completed",
                priority=TicketPriority.MEDIUM,
                status=TicketStatus.DONE,
            )
        ],
    )


@pytest.fixture
async def e2e_app_with_ac_ticket(e2e_project):
    """Create a KaganApp with a ticket that has acceptance criteria."""
    return await _create_e2e_app_with_tickets(
        e2e_project,
        [
            Ticket.create(
                title="Task with acceptance criteria",
                description="A task with defined acceptance criteria",
                priority=TicketPriority.HIGH,
                status=TicketStatus.BACKLOG,
                acceptance_criteria=["User can login", "Error messages shown"],
            )
        ],
    )


# =============================================================================
# Integration Test Fixtures
# =============================================================================


@pytest.fixture
def scheduler(state_manager, mock_worktree_manager, config):
    """Create a scheduler instance with default config (auto_merge=false).

    Shared by: test_scheduler_*.py
    """
    from kagan.agents.scheduler import Scheduler

    changed_callback = MagicMock()
    return Scheduler(
        state_manager=state_manager,
        worktree_manager=mock_worktree_manager,
        config=config,
        on_ticket_changed=changed_callback,
    )


@pytest.fixture
def auto_merge_config():
    """Create a test config with auto_merge enabled.

    Shared by: test_scheduler_automerge.py, test_scheduler_automerge_extended.py
    """
    from kagan.config import AgentConfig, GeneralConfig, KaganConfig

    return KaganConfig(
        general=GeneralConfig(
            auto_start=True,
            auto_merge=True,
            max_concurrent_agents=2,
            max_iterations=3,
            iteration_delay_seconds=0.01,
            default_worker_agent="test",
            default_base_branch="main",
        ),
        agents={
            "test": AgentConfig(
                identity="test.agent",
                name="Test Agent",
                short_name="test",
                run_command={"*": "echo test"},
            )
        },
    )


@pytest.fixture
def mock_review_agent():
    """Create a mock agent for review that returns approve signal.

    Shared by: test_scheduler_automerge.py, test_scheduler_automerge_extended.py
    """
    agent = MagicMock()
    agent.set_auto_approve = MagicMock()
    agent.start = MagicMock()
    agent.wait_ready = AsyncMock()
    agent.send_prompt = AsyncMock()
    agent.get_response_text = MagicMock(
        return_value='Looks good! <approve summary="Implementation complete"/>'
    )
    agent.stop = AsyncMock()
    return agent


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
    monkeypatch.setattr("kagan.sessions.tmux.run_tmux", fake)
    monkeypatch.setattr("kagan.sessions.manager.run_tmux", fake)


@pytest.fixture
def mock_tmux(monkeypatch):
    """Intercept tmux calls and return session state for assertions."""
    sessions: dict = {}
    monkeypatch.setattr("kagan.sessions.manager.run_tmux", _create_fake_tmux(sessions))
    return sessions
