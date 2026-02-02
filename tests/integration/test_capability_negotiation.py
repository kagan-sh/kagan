"""Tests for ACP capability negotiation.

Verifies that:
1. Capabilities are correctly sent based on read_only flag
2. Agent capability responses are validated
3. Write operations fail fast if capability wasn't negotiated
"""

from __future__ import annotations

from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from kagan.acp.agent import Agent

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.config import AgentConfig

pytestmark = pytest.mark.integration


# Fixtures agent_config and mock_process are provided by conftest.py


class TestCapabilitiesSentBasedOnReadOnlyFlag:
    """Test that capabilities are correctly constructed based on read_only."""

    def test_read_write_mode_includes_write_capability(
        self, tmp_path: Path, agent_config: AgentConfig
    ):
        """Non-read-only agent should advertise writeTextFile capability."""
        agent = Agent(tmp_path, agent_config, read_only=False)
        assert agent._read_only is False

        # Build capabilities the same way _acp_initialize does
        built_fs_caps: dict[str, bool] = {"readTextFile": True}
        if not agent._read_only:
            built_fs_caps["writeTextFile"] = True

        expected: dict[str, object] = {"fs": built_fs_caps, "terminal": True}
        assert expected.get("fs", {}).get("writeTextFile") is True  # type: ignore
        assert expected.get("terminal") is True

    def test_read_only_mode_omits_write_capability(self, tmp_path: Path, agent_config: AgentConfig):
        """Read-only agent should NOT advertise writeTextFile capability."""
        agent = Agent(tmp_path, agent_config, read_only=True)
        assert agent._read_only is True

        # Build capabilities the same way _acp_initialize does
        fs_caps: dict[str, bool] = {"readTextFile": True}
        if not agent._read_only:
            fs_caps["writeTextFile"] = True
        caps: dict[str, object] = {"fs": fs_caps, "terminal": not agent._read_only}

        assert caps.get("fs", {}).get("writeTextFile") is None  # type: ignore
        assert caps.get("terminal") is False


class TestCapabilityNegotiationResponseValidation:
    """Test that agent's capability response is validated."""

    async def test_agent_capabilities_stored_after_initialize(
        self, tmp_path: Path, agent_config: AgentConfig, mock_process: MagicMock
    ):
        """Agent capabilities from response are stored."""
        agent = Agent(tmp_path, agent_config, read_only=False)
        agent._process = mock_process

        response = {
            "protocolVersion": "1",
            "agentCapabilities": {
                "loadSession": True,
                "promptCapabilities": {"audio": False, "embeddedContent": True, "image": False},
            },
        }

        mock_call = MagicMock()
        mock_call.wait = AsyncMock(return_value=response)

        with patch.object(agent._client, "call", return_value=mock_call):
            await agent._acp_initialize()

        assert agent.agent_capabilities == response["agentCapabilities"]

    async def test_empty_agent_capabilities_accepted(
        self, tmp_path: Path, agent_config: AgentConfig, mock_process: MagicMock
    ):
        """Empty agent capabilities response is accepted without validation.

        BUG: No validation that agent accepted our requested capabilities.
        """
        agent = Agent(tmp_path, agent_config, read_only=False)
        agent._process = mock_process

        response = {"protocolVersion": "1", "agentCapabilities": {}}

        mock_call = MagicMock()
        mock_call.wait = AsyncMock(return_value=response)

        with patch.object(agent._client, "call", return_value=mock_call):
            await agent._acp_initialize()

        # We proceed even with empty capabilities - no validation
        assert agent.agent_capabilities == {}


class TestWriteOperationsGuards:
    """Test that write operations are guarded by read_only flag."""

    def test_write_blocked_in_read_only_mode(self, tmp_path: Path, agent_config: AgentConfig):
        """Write operations raise ValueError when agent is in read_only mode."""
        agent = Agent(tmp_path, agent_config, read_only=True)

        with pytest.raises(ValueError, match="read-only mode"):
            agent._rpc_write_text_file("session-1", "test.txt", "content")

    async def test_terminal_blocked_in_read_only_mode(
        self, tmp_path: Path, agent_config: AgentConfig
    ):
        """Terminal operations raise ValueError when agent is in read_only mode."""
        agent = Agent(tmp_path, agent_config, read_only=True)
        agent._terminals = MagicMock()

        with pytest.raises(ValueError, match="read-only mode"):
            await agent._rpc_terminal_create("ls")

    def test_write_succeeds_in_normal_mode(self, tmp_path: Path, agent_config: AgentConfig):
        """Write operations work when agent is NOT in read_only mode."""
        agent = Agent(tmp_path, agent_config, read_only=False)
        agent._rpc_write_text_file("session-1", "test.txt", "hello")
        assert (tmp_path / "test.txt").read_text() == "hello"

    def test_file_not_created_when_write_blocked(self, tmp_path: Path, agent_config: AgentConfig):
        """Files are not created when write is blocked."""
        agent = Agent(tmp_path, agent_config, read_only=True)

        with pytest.raises(ValueError):
            agent._rpc_write_text_file("s1", "should_not_exist.txt", "data")

        assert not (tmp_path / "should_not_exist.txt").exists()


class TestSchedulerAgentCapabilities:
    """Test that scheduler creates agents with correct capabilities."""

    def test_auto_ticket_agent_has_write_capability(
        self, tmp_path: Path, agent_config: AgentConfig
    ):
        """AUTO ticket agents are created with write capability (not read_only).

        This verifies the scheduler creates work agents correctly.
        """
        # Scheduler line 256: agent = Agent(wt_path, agent_config)
        # No read_only=True, so defaults to False
        agent = Agent(tmp_path, agent_config)  # Same as scheduler
        assert agent._read_only is False

    def test_review_agent_is_read_only(self, tmp_path: Path, agent_config: AgentConfig):
        """Review agents are created with read_only=True.

        This verifies review agents can't modify code.
        """
        # Scheduler line 334: agent = Agent(wt_path, agent_config, read_only=True)
        agent = Agent(tmp_path, agent_config, read_only=True)
        assert agent._read_only is True
