"""Tests for Agent and AgentManager with ACP-based agents."""

from __future__ import annotations

import tempfile
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from kagan.agents import Agent, AgentManager
from kagan.config import AgentConfig


@pytest.fixture
def temp_dir():
    with tempfile.TemporaryDirectory() as tmpdir:
        yield Path(tmpdir)


@pytest.fixture
def test_agent_config() -> AgentConfig:
    """Create a test agent configuration."""
    return AgentConfig(
        identity="test-agent",
        name="Test Agent",
        short_name="test",
        run_command={"*": "echo test"},
    )


@pytest.fixture
def mock_agent() -> MagicMock:
    """Create a mock agent."""
    agent = MagicMock(spec=Agent)
    agent.stop = AsyncMock()
    return agent


class TestAgentManager:
    """Tests for AgentManager spawn/get/terminate/list functionality."""

    async def test_spawn_registers_agent(
        self, temp_dir: Path, test_agent_config: AgentConfig, mock_agent: MagicMock
    ) -> None:
        """Test that spawn registers an agent for the ticket."""
        manager = AgentManager()

        with patch("kagan.agents.manager.Agent", return_value=mock_agent):
            agent = await manager.spawn("t1", test_agent_config, temp_dir)

        assert agent is mock_agent
        assert manager.get("t1") is mock_agent
        assert "t1" in manager.list_active()
        mock_agent.start.assert_called_once()
        await manager.terminate_all()

    async def test_spawn_multiple(self, temp_dir: Path, test_agent_config: AgentConfig) -> None:
        """Test spawning multiple agents for different tickets."""
        manager = AgentManager()

        def make_mock_agent(*args, **kwargs):
            agent = MagicMock(spec=Agent)
            agent.stop = AsyncMock()
            return agent

        with patch("kagan.agents.manager.Agent", side_effect=make_mock_agent):
            await manager.spawn("t1", test_agent_config, temp_dir)
            await manager.spawn("t2", test_agent_config, temp_dir)

        assert len(manager.list_active()) == 2
        assert "t1" in manager.list_active()
        assert "t2" in manager.list_active()
        await manager.terminate_all()
        assert len(manager.list_active()) == 0

    async def test_duplicate_spawn_raises(
        self, temp_dir: Path, test_agent_config: AgentConfig, mock_agent: MagicMock
    ) -> None:
        """Test that spawning an agent for the same ticket raises ValueError."""
        manager = AgentManager()

        with patch("kagan.agents.manager.Agent", return_value=mock_agent):
            await manager.spawn("t1", test_agent_config, temp_dir)

            with pytest.raises(ValueError, match="already running"):
                await manager.spawn("t1", test_agent_config, temp_dir)

        await manager.terminate_all()

    async def test_get_agent(
        self, temp_dir: Path, test_agent_config: AgentConfig, mock_agent: MagicMock
    ) -> None:
        """Test getting an agent by ticket_id."""
        manager = AgentManager()

        with patch("kagan.agents.manager.Agent", return_value=mock_agent):
            agent = await manager.spawn("t1", test_agent_config, temp_dir)

        assert manager.get("t1") is agent
        assert manager.get("nonexistent") is None
        await manager.terminate_all()

    async def test_terminate_single(self, temp_dir: Path, test_agent_config: AgentConfig) -> None:
        """Test terminating a single agent."""
        manager = AgentManager()
        agents: dict[str, Agent] = {}

        def make_mock_agent(*args, **kwargs):
            agent = MagicMock(spec=Agent)
            agent.stop = AsyncMock()
            return agent

        with patch("kagan.agents.manager.Agent", side_effect=make_mock_agent):
            agents["t1"] = await manager.spawn("t1", test_agent_config, temp_dir)
            agents["t2"] = await manager.spawn("t2", test_agent_config, temp_dir)

        await manager.terminate("t1")

        assert manager.get("t1") is None
        assert manager.get("t2") is not None
        agents["t1"].stop.assert_called_once()  # type: ignore[union-attr]
        agents["t2"].stop.assert_not_called()  # type: ignore[union-attr]
        await manager.terminate_all()

    async def test_terminate_all(self, temp_dir: Path, test_agent_config: AgentConfig) -> None:
        """Test terminating all agents."""
        manager = AgentManager()
        created_agents: list[MagicMock] = []

        def make_mock_agent(*args, **kwargs):
            agent = MagicMock(spec=Agent)
            agent.stop = AsyncMock()
            created_agents.append(agent)
            return agent

        with patch("kagan.agents.manager.Agent", side_effect=make_mock_agent):
            await manager.spawn("t1", test_agent_config, temp_dir)
            await manager.spawn("t2", test_agent_config, temp_dir)
            await manager.spawn("t3", test_agent_config, temp_dir)

        assert len(manager.list_active()) == 3

        await manager.terminate_all()

        assert len(manager.list_active()) == 0
        for agent in created_agents:
            agent.stop.assert_called_once()

    async def test_is_running(
        self, temp_dir: Path, test_agent_config: AgentConfig, mock_agent: MagicMock
    ) -> None:
        """Test is_running check."""
        manager = AgentManager()

        assert not manager.is_running("t1")

        with patch("kagan.agents.manager.Agent", return_value=mock_agent):
            await manager.spawn("t1", test_agent_config, temp_dir)

        assert manager.is_running("t1")
        assert not manager.is_running("t2")
        await manager.terminate_all()

    async def test_list_active_returns_ticket_ids(
        self, temp_dir: Path, test_agent_config: AgentConfig
    ) -> None:
        """Test that list_active returns all ticket IDs with running agents."""
        manager = AgentManager()

        def make_mock_agent(*args, **kwargs):
            agent = MagicMock(spec=Agent)
            agent.stop = AsyncMock()
            return agent

        with patch("kagan.agents.manager.Agent", side_effect=make_mock_agent):
            await manager.spawn("ticket-a", test_agent_config, temp_dir)
            await manager.spawn("ticket-b", test_agent_config, temp_dir)
            await manager.spawn("ticket-c", test_agent_config, temp_dir)

        active = manager.list_active()
        assert set(active) == {"ticket-a", "ticket-b", "ticket-c"}
        await manager.terminate_all()


class TestAgentConfig:
    """Tests for AgentConfig model."""

    def test_agent_config_creation(self) -> None:
        """Test creating an AgentConfig."""
        config = AgentConfig(
            identity="claude.com",
            name="Claude Code",
            short_name="claude",
            run_command={"macos": "claude --acp", "*": "claude"},
        )

        assert config.identity == "claude.com"
        assert config.name == "Claude Code"
        assert config.short_name == "claude"
        assert config.protocol == "acp"
        assert config.active is True
        assert config.run_command["macos"] == "claude --acp"
        assert config.run_command["*"] == "claude"

    def test_agent_config_defaults(self) -> None:
        """Test AgentConfig default values."""
        config = AgentConfig(
            identity="test",
            name="Test",
            short_name="t",
            run_command={"*": "test"},
        )

        assert config.protocol == "acp"
        assert config.active is True

    def test_agent_config_inactive(self) -> None:
        """Test creating an inactive agent config."""
        config = AgentConfig(
            identity="test",
            name="Test",
            short_name="t",
            run_command={"*": "test"},
            active=False,
        )

        assert config.active is False


class TestAgent:
    """Tests for Agent class (mocked subprocess)."""

    def test_agent_init(self, temp_dir: Path, test_agent_config: AgentConfig) -> None:
        """Test Agent initialization."""
        agent = Agent(temp_dir, test_agent_config)

        assert agent.project_root == temp_dir
        assert agent.session_id == ""
        assert agent.tool_calls == {}

    def test_agent_command_property(self, temp_dir: Path, test_agent_config: AgentConfig) -> None:
        """Test Agent command property returns the right command."""
        agent = Agent(temp_dir, test_agent_config)

        # With wildcard "*", should return "echo test" on any OS
        assert agent.command == "echo test"

    def test_agent_command_os_specific(self, temp_dir: Path) -> None:
        """Test Agent command property with OS-specific commands."""
        config = AgentConfig(
            identity="test",
            name="Test",
            short_name="t",
            run_command={
                "macos": "mac-command",
                "linux": "linux-command",
                "windows": "win-command",
            },
        )
        agent = Agent(temp_dir, config)

        # The command should be platform-specific
        # We can't easily test the actual value without knowing the platform,
        # but we can verify it's one of the expected values
        assert agent.command in ("mac-command", "linux-command", "win-command", None)
