"""Core service mock factories.

Provides mock objects for workspace services, agent processes, agent
configuration, and tmux terminal backends.
"""

from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace
from typing import TYPE_CHECKING, Any
from unittest.mock import AsyncMock, MagicMock

from kagan.core.tmux import TmuxError

if TYPE_CHECKING:
    from kagan.core.config import AgentConfig, KaganConfig


def create_mock_workspace_service() -> MagicMock:
    """Create a mock WorkspaceService with async methods."""
    manager = MagicMock()
    workspace_stub = SimpleNamespace(id="workspace-test")
    manager.get_path = AsyncMock(return_value=Path("/tmp/worktree"))
    manager.create = AsyncMock(return_value=Path("/tmp/worktree"))
    manager.delete = AsyncMock()
    manager.list_workspaces = AsyncMock(return_value=[workspace_stub])
    manager.get_workspace_repos = AsyncMock(return_value=[])
    manager.get_commit_log = AsyncMock(return_value=["feat: initial"])
    manager.get_diff_stats = AsyncMock(return_value="1 file changed")
    manager.prepare_merge_conflicts = AsyncMock(return_value=(True, "Merge conflicts prepared"))
    manager.get_merge_worktree_path = AsyncMock(return_value=Path("/tmp/merge-worktree"))
    manager.get_files_changed_on_base = AsyncMock(return_value=[])
    manager.rebase_onto_base = AsyncMock(return_value=(True, "", []))
    return manager


def create_mock_agent(response: str = "Done! <complete/>") -> MagicMock:
    """Create a mock ACP agent with configurable response."""
    from kagan.core.acp.messages import AgentBuffers

    agent = MagicMock()
    buffers = AgentBuffers()
    buffers.append_response(response)
    agent._read_only = False
    agent.set_auto_approve = MagicMock()
    agent.start = MagicMock()
    agent.wait_ready = AsyncMock()
    agent.send_prompt = AsyncMock()
    agent.get_response_text = MagicMock(side_effect=buffers.get_response_text)
    agent.get_messages = MagicMock(side_effect=lambda: list(buffers.messages))
    agent.stop = AsyncMock()
    return agent


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


def create_test_agent_config(
    identity: str = "test.agent",
    name: str = "Test Agent",
    short_name: str = "test",
    run_command: str = "echo test",
) -> AgentConfig:
    """Create a minimal AgentConfig for testing."""
    from kagan.core.config import AgentConfig

    return AgentConfig(
        identity=identity,
        name=name,
        short_name=short_name,
        run_command={"*": run_command},
    )


def create_test_config(
    auto_review: bool = True,
    max_concurrent: int = 2,
) -> KaganConfig:
    """Create a KaganConfig for testing."""
    from kagan.core.config import AgentConfig, GeneralConfig, KaganConfig

    return KaganConfig(
        general=GeneralConfig(
            auto_review=auto_review,
            max_concurrent_agents=max_concurrent,
            default_worker_agent="test",
            default_base_branch="main",
            default_pair_terminal_backend="tmux",
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


def create_fake_tmux(sessions: dict[str, dict[str, Any]], *, strict: bool = False) -> Any:
    """Create a fake tmux run function for testing.

    Args:
        sessions: Dictionary to track created sessions
        strict: If True, raise TmuxError for invalid operations like
            kill-session or send-keys on non-existent sessions
    """

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
            idx = args_list.index("-t")
            name = args_list[idx + 1] if idx + 1 < len(args_list) else None
            if strict and name and name not in sessions:
                raise TmuxError(f"session not found: {name}")
            if name:
                sessions.pop(name, None)
        elif command == "send-keys" and "-t" in args_list:
            idx = args_list.index("-t")
            name = args_list[idx + 1]
            if strict and name not in sessions:
                raise TmuxError(f"session not found: {name}")
            keys = args_list[idx + 2] if idx + 2 < len(args_list) else ""
            if name in sessions:
                sessions[name]["sent_keys"].append(keys)
        elif command == "list-sessions":
            return "\n".join(sorted(sessions.keys()))
        return ""

    return fake_run_tmux


def install_fake_tmux(
    monkeypatch: Any,
    sessions: dict[str, dict[str, Any]] | None = None,
    *,
    strict: bool = False,
) -> dict[str, dict[str, Any]]:
    """Install fake tmux handlers and return the backing session map."""
    active_sessions = sessions if sessions is not None else {}
    fake_tmux = create_fake_tmux(active_sessions, strict=strict)
    monkeypatch.setattr("kagan.core.tmux.run_tmux", fake_tmux)
    monkeypatch.setattr("kagan.core.services.sessions.run_tmux", fake_tmux)
    return active_sessions
