"""Pytest fixtures for Kagan tests."""

from __future__ import annotations

import asyncio
import contextlib
import tempfile
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

import pytest

from kagan.app import KaganApp
from kagan.database.manager import StateManager
from tests.helpers.git import init_git_repo_with_commit
from tests.helpers.mocks import (
    create_mock_agent,
    create_mock_session_manager,
    create_mock_worktree_manager,
    create_test_config,
)

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
async def state_manager_factory(tmp_path: Path):
    """Factory fixture to create state managers with access to db_path.

    Use this when you need to verify db file creation/existence.
    """
    managers: list[StateManager] = []

    async def _factory():
        db_path = tmp_path / "test.db"
        manager = StateManager(db_path)
        await manager.initialize()
        managers.append(manager)
        return manager, db_path

    yield _factory

    # Cleanup all created managers in async context
    for manager in managers:
        with contextlib.suppress(RuntimeError):
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


# =============================================================================
# E2E Test Fixtures
# =============================================================================


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
async def e2e_app_with_review_ticket_and_worktree(e2e_project):
    """App with REVIEW ticket that has a worktree with commits."""
    from kagan.database.models import TicketCreate, TicketPriority, TicketStatus

    manager = StateManager(e2e_project.db)
    await manager.initialize()

    # Create a REVIEW ticket
    ticket = await manager.create_ticket(
        TicketCreate(
            title="Feature with changes",
            description="A feature ready for review with commits",
            priority=TicketPriority.HIGH,
            status=TicketStatus.REVIEW,
        )
    )

    await manager.close()

    # Create branch from main
    branch_name = f"kagan/{ticket.id}"
    proc = await asyncio.create_subprocess_exec(
        "git",
        "branch",
        branch_name,
        cwd=e2e_project.root,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    await proc.communicate()

    # Create worktree directory
    worktree_path = e2e_project.root / ".kagan" / "worktrees" / ticket.id
    worktree_path.parent.mkdir(parents=True, exist_ok=True)

    proc = await asyncio.create_subprocess_exec(
        "git",
        "worktree",
        "add",
        str(worktree_path),
        branch_name,
        cwd=e2e_project.root,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    await proc.communicate()

    # Make a change and commit in the worktree
    feature_file = worktree_path / "feature.py"
    feature_file.write_text("# New feature\ndef new_feature():\n    return True\n")

    proc = await asyncio.create_subprocess_exec(
        "git",
        "add",
        ".",
        cwd=worktree_path,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    await proc.communicate()

    proc = await asyncio.create_subprocess_exec(
        "git",
        "commit",
        "-m",
        f"Add feature for {ticket.id}",
        cwd=worktree_path,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    await proc.communicate()

    app = KaganApp(
        db_path=e2e_project.db,
        config_path=e2e_project.config,
        lock_path=None,
    )
    return app


@pytest.fixture
async def e2e_fresh_project(tmp_path: Path):
    """Create a project directory WITHOUT git initialization.

    This simulates a user running kagan in a completely empty folder,
    which triggers kagan to initialize git itself.
    """
    project = tmp_path / "fresh_project"
    project.mkdir()

    # NO git init - kagan should do this itself
    # Just create .kagan config so it skips welcome screen
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
async def e2e_app_fresh(e2e_fresh_project):
    """Create a KaganApp for a fresh project without git.

    Kagan will initialize git when it detects no repo exists.
    """
    app = KaganApp(
        db_path=e2e_fresh_project.db,
        config_path=e2e_fresh_project.config,
        lock_path=None,
    )
    return app


@pytest.fixture
async def e2e_app_with_tickets(e2e_project):
    """Create a KaganApp with pre-populated tickets for testing.

    Tickets are created via StateManager before the app runs,
    simulating a user who has already been using Kagan.
    """
    from kagan.database.models import TicketCreate, TicketPriority, TicketStatus

    # Initialize state manager and create tickets
    manager = StateManager(e2e_project.db)
    await manager.initialize()

    await manager.create_ticket(
        TicketCreate(
            title="Backlog task",
            description="A task in backlog",
            priority=TicketPriority.LOW,
            status=TicketStatus.BACKLOG,
        )
    )
    await manager.create_ticket(
        TicketCreate(
            title="In progress task",
            description="Currently working",
            priority=TicketPriority.HIGH,
            status=TicketStatus.IN_PROGRESS,
        )
    )
    await manager.create_ticket(
        TicketCreate(
            title="Review task",
            description="Ready for review",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.REVIEW,
        )
    )

    await manager.close()

    # Create app pointing to same DB
    app = KaganApp(
        db_path=e2e_project.db,
        config_path=e2e_project.config,
        lock_path=None,
    )
    return app


@pytest.fixture
async def e2e_app_with_auto_ticket(e2e_project):
    """Create a KaganApp with an AUTO ticket in IN_PROGRESS for testing.

    This is used to test AUTO ticket movement restrictions.
    """
    from kagan.database.models import TicketCreate, TicketPriority, TicketStatus, TicketType

    manager = StateManager(e2e_project.db)
    await manager.initialize()

    # Create an AUTO ticket in IN_PROGRESS
    await manager.create_ticket(
        TicketCreate(
            title="Auto task in progress",
            description="An AUTO task currently being worked on by agent",
            priority=TicketPriority.HIGH,
            status=TicketStatus.IN_PROGRESS,
            ticket_type=TicketType.AUTO,
        )
    )

    await manager.close()

    app = KaganApp(
        db_path=e2e_project.db,
        config_path=e2e_project.config,
        lock_path=None,
    )
    return app


@pytest.fixture
async def e2e_app_with_done_ticket(e2e_project):
    """Create a KaganApp with a ticket in DONE status for testing.

    This is used to test DONE -> BACKLOG jump behavior.
    """
    from kagan.database.models import TicketCreate, TicketPriority, TicketStatus

    manager = StateManager(e2e_project.db)
    await manager.initialize()

    # Create a DONE ticket
    await manager.create_ticket(
        TicketCreate(
            title="Completed task",
            description="A task that has been completed",
            priority=TicketPriority.MEDIUM,
            status=TicketStatus.DONE,
        )
    )

    await manager.close()

    app = KaganApp(
        db_path=e2e_project.db,
        config_path=e2e_project.config,
        lock_path=None,
    )
    return app


@pytest.fixture
async def e2e_app_with_ac_ticket(e2e_project):
    """Create a KaganApp with a ticket that has acceptance criteria."""
    from kagan.database.models import TicketCreate, TicketPriority, TicketStatus

    manager = StateManager(e2e_project.db)
    await manager.initialize()

    await manager.create_ticket(
        TicketCreate(
            title="Task with acceptance criteria",
            description="A task with defined acceptance criteria",
            priority=TicketPriority.HIGH,
            status=TicketStatus.BACKLOG,
            acceptance_criteria=["User can login", "Error messages shown"],
        )
    )

    await manager.close()

    app = KaganApp(
        db_path=e2e_project.db,
        config_path=e2e_project.config,
        lock_path=None,
    )
    return app


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


@pytest.fixture
def mock_tmux(monkeypatch):
    """Intercept tmux subprocess calls.

    Shared by: test_sessions.py, test_sessions_lifecycle.py
    """
    from typing import Any

    sessions: dict[str, dict[str, Any]] = {}

    async def fake_run_tmux(*args: str) -> str:
        command = args[0]
        if command == "new-session":
            name = args[args.index("-s") + 1]
            cwd = args[args.index("-c") + 1]
            env: dict[str, str] = {}
            for idx, value in enumerate(args):
                if value == "-e" and idx + 1 < len(args):
                    key, _, env_value = args[idx + 1].partition("=")
                    env[key] = env_value
            sessions[name] = {"cwd": cwd, "env": env, "sent_keys": []}
            return ""
        if command == "send-keys":
            name = args[args.index("-t") + 1]
            key_text = args[args.index("-t") + 2]
            if name in sessions:
                sessions[name]["sent_keys"].append(key_text)
            return ""
        if command == "list-sessions":
            return "\n".join(sorted(sessions.keys()))
        if command == "kill-session":
            name = args[args.index("-t") + 1]
            sessions.pop(name, None)
            return ""
        return ""

    monkeypatch.setattr("kagan.sessions.manager.run_tmux", fake_run_tmux)
    return sessions
