"""Integration tests for Agent lifecycle: init, start, stop, response handling."""

from __future__ import annotations

from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from kagan.acp.agent import Agent
from kagan.acp.buffers import AgentBuffers

if TYPE_CHECKING:
    from pathlib import Path

pytestmark = pytest.mark.integration


class TestAgentInitialization:
    """Tests for Agent initialization with config."""

    def test_agent_initialization_with_config(self, git_repo: Path, config):
        """Agent initializes correctly with valid config and project root."""
        agent_config = config.agents["test"]
        agent = Agent(git_repo, agent_config)

        assert agent.project_root == git_repo
        assert agent._agent_config == agent_config
        assert agent._read_only is False
        assert agent._process is None
        assert agent.session_id == ""
        assert agent.tool_calls == {}

    def test_agent_initialization_read_only_mode(self, git_repo: Path, config):
        """Agent initializes with read_only=True restricts capabilities."""
        agent_config = config.agents["test"]
        agent = Agent(git_repo, agent_config, read_only=True)

        assert agent._read_only is True

    def test_agent_has_buffers(self, git_repo: Path, config):
        """Agent initializes with buffers for response and message management."""
        agent_config = config.agents["test"]
        agent = Agent(git_repo, agent_config)

        assert isinstance(agent._buffers, AgentBuffers)


class TestAgentAutoApprove:
    """Tests for agent auto_approve flag."""

    def test_agent_set_auto_approve_enabled(self, git_repo: Path, config):
        """Setting auto_approve to True enables auto-approval mode."""
        agent = Agent(git_repo, config.agents["test"])

        agent.set_auto_approve(True)

        assert agent._auto_approve is True

    def test_agent_set_auto_approve_disabled(self, git_repo: Path, config):
        """Setting auto_approve to False disables auto-approval mode."""
        agent = Agent(git_repo, config.agents["test"])
        agent.set_auto_approve(True)

        agent.set_auto_approve(False)

        assert agent._auto_approve is False


class TestAgentResponseText:
    """Tests for response text accumulation and retrieval."""

    def test_agent_get_response_text_empty(self, git_repo: Path, config):
        """get_response_text returns empty string when no content accumulated."""
        agent = Agent(git_repo, config.agents["test"])

        result = agent.get_response_text()

        assert result == ""

    def test_agent_get_response_text_with_content(self, git_repo: Path, config):
        """get_response_text returns accumulated response from buffer."""
        agent = Agent(git_repo, config.agents["test"])
        agent._buffers.append_response("Hello ")
        agent._buffers.append_response("World!")

        result = agent.get_response_text()

        assert result == "Hello World!"


class TestAgentStartStop:
    """Tests for agent start/stop lifecycle."""

    async def test_agent_start_creates_task(self, git_repo: Path, config):
        """start() creates an async task for the agent loop."""
        agent = Agent(git_repo, config.agents["test"])

        with patch("asyncio.create_task") as mock_create_task:
            # Create a mock task that can be awaited
            mock_task = MagicMock()
            mock_create_task.return_value = mock_task

            # Capture the coroutine passed to create_task
            coro_arg = None

            def capture_coro(coro):
                nonlocal coro_arg
                coro_arg = coro
                return mock_task

            mock_create_task.side_effect = capture_coro
            agent.start()

            mock_create_task.assert_called_once()
            assert agent._agent_task is not None

            # Clean up the captured coroutine to avoid warning
            if coro_arg is not None:
                coro_arg.close()

    async def test_agent_stop_cleans_up_resources(self, git_repo: Path, config):
        """stop() cleans up terminals, buffers, and terminates process."""
        agent = Agent(git_repo, config.agents["test"])
        agent._buffers.append_response("test response")

        # Mock the process
        mock_process = MagicMock()
        mock_process.returncode = None
        mock_process.terminate = MagicMock()
        mock_process.wait = AsyncMock(return_value=0)
        agent._process = mock_process

        await agent.stop()

        mock_process.terminate.assert_called_once()
        assert agent._buffers.get_response_text() == ""

    async def test_agent_stop_without_process(self, git_repo: Path, config):
        """stop() handles case when no process is running."""
        agent = Agent(git_repo, config.agents["test"])
        agent._buffers.append_response("test")

        await agent.stop()

        # Should not raise, just cleans up
        assert agent._buffers.get_response_text() == ""


class TestAgentReadOnlyMode:
    """Tests for read_only agent capability restrictions."""

    def test_read_only_agent_cannot_write_files(self, git_repo: Path, config):
        """Read-only agent blocks file write operations via RPC handler."""
        from kagan.acp import rpc

        agent = Agent(git_repo, config.agents["test"], read_only=True)

        with pytest.raises(ValueError, match="not permitted in read-only mode"):
            rpc.handle_write_text_file(agent, "session-1", "file.txt", "content")

    async def test_read_only_agent_cannot_create_terminal(self, git_repo: Path, config):
        """Read-only agent blocks terminal creation via RPC handler."""
        from kagan.acp import rpc

        agent = Agent(git_repo, config.agents["test"], read_only=True)

        with pytest.raises(ValueError, match="not permitted in read-only mode"):
            await rpc.handle_terminal_create(agent, "echo test")

    def test_read_only_agent_can_read_files(self, git_repo: Path, config):
        """Read-only agent allows file read operations."""
        from kagan.acp import rpc

        # Create a test file
        test_file = git_repo / "test.txt"
        test_file.write_text("readable content")

        agent = Agent(git_repo, config.agents["test"], read_only=True)

        result = rpc.handle_read_text_file(agent, "session-1", "test.txt")

        assert result["content"] == "readable content"


class TestAgentMessageTarget:
    """Tests for message target and buffering."""

    def test_set_message_target(self, git_repo: Path, config):
        """set_message_target sets the target for posting messages."""
        agent = Agent(git_repo, config.agents["test"])
        mock_target = MagicMock()

        agent.set_message_target(mock_target)

        assert agent._message_target == mock_target

    def test_set_message_target_replays_buffered_messages(self, git_repo: Path, config):
        """Setting message target replays any buffered messages."""
        from kagan.acp import messages

        agent = Agent(git_repo, config.agents["test"])
        # Buffer a message before target is set
        agent._buffers.buffer_message(messages.AgentUpdate("text", "buffered"))

        mock_target = MagicMock()
        mock_target.post_message = MagicMock(return_value=True)

        agent.set_message_target(mock_target)

        # Buffered message should be replayed
        mock_target.post_message.assert_called_once()

    def test_post_message_buffers_when_no_target(self, git_repo: Path, config):
        """post_message buffers messages when no target is set."""
        from kagan.acp import messages

        agent = Agent(git_repo, config.agents["test"])
        msg = messages.AgentUpdate("text", "test")

        result = agent.post_message(msg)

        assert result is False
        assert len(agent._buffers.messages) == 1
