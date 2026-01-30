"""Mock factories for tests."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock

if TYPE_CHECKING:
    from kagan.config import KaganConfig


def create_mock_worktree_manager() -> MagicMock:
    """Create a mock WorktreeManager with async methods."""
    from kagan.agents.worktree import WorktreeManager

    manager = MagicMock(spec=WorktreeManager)
    manager.get_path = AsyncMock(return_value=Path("/tmp/worktree"))
    manager.create = AsyncMock(return_value=Path("/tmp/worktree"))
    manager.delete = AsyncMock()
    manager.list_all = AsyncMock(return_value=[])
    manager.get_commit_log = AsyncMock(return_value=["feat: initial"])
    manager.get_diff_stats = AsyncMock(return_value="1 file changed")
    manager.merge_to_main = AsyncMock(return_value=(True, "Merged"))
    return manager


def create_mock_agent(response: str = "Done! <complete/>") -> MagicMock:
    """Create a mock ACP agent with configurable response."""
    agent = MagicMock()
    agent._read_only = False  # Default to normal (non-read-only) mode
    agent.set_auto_approve = MagicMock()
    agent.start = MagicMock()
    agent.wait_ready = AsyncMock()
    agent.send_prompt = AsyncMock()
    agent.get_response_text = MagicMock(return_value=response)
    agent.stop = AsyncMock()
    return agent


def create_mock_session_manager() -> MagicMock:
    """Create a mock SessionManager."""
    manager = MagicMock()
    manager.create_session = AsyncMock(return_value="session-123")
    manager.kill_session = AsyncMock()
    manager.list_sessions = AsyncMock(return_value=[])
    manager.send_keys = AsyncMock()
    return manager


def create_mock_process(pid: int = 12345, returncode: int | None = None) -> MagicMock:
    """Create a mock asyncio subprocess."""
    proc = MagicMock()
    proc.pid = pid
    proc.returncode = returncode
    proc.stdout = MagicMock()
    proc.stdout.readline = AsyncMock(return_value=b"")
    proc.stderr = MagicMock()
    proc.stderr.readline = AsyncMock(return_value=b"")
    proc.wait = AsyncMock(return_value=0)
    proc.terminate = MagicMock()
    proc.kill = MagicMock()
    proc.communicate = AsyncMock(return_value=(b"", b""))
    return proc


def create_test_config(
    auto_start: bool = True,
    auto_merge: bool = False,
    max_concurrent: int = 2,
    max_iterations: int = 3,
) -> KaganConfig:
    """Create a KaganConfig for testing."""
    from kagan.config import AgentConfig, GeneralConfig, KaganConfig

    return KaganConfig(
        general=GeneralConfig(
            auto_start=auto_start,
            auto_merge=auto_merge,
            max_concurrent_agents=max_concurrent,
            max_iterations=max_iterations,
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
