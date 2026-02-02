"""Tests for ACP RPC handlers (now inlined into Agent class)."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, cast
from unittest.mock import MagicMock, Mock

import pytest

from kagan.acp import messages
from kagan.acp.agent import Agent

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.acp.protocol import SessionUpdate
    from kagan.config import AgentConfig

pytestmark = pytest.mark.integration


def create_test_agent(tmp_path: Path, agent_config: AgentConfig) -> Agent:
    """Create an agent for testing RPC handlers."""
    agent = Agent(tmp_path, agent_config, read_only=False)
    agent._buffers = Mock()  # type: ignore[assignment]
    agent._message_target = Mock()  # type: ignore[assignment]
    agent.post_message = MagicMock(return_value=True)  # type: ignore[method-assign]
    return agent


class TestHandleSessionUpdateAgentMessage:
    def test_agent_message_chunk_appends_to_buffer(self, tmp_path: Path, agent_config: AgentConfig):
        agent = create_test_agent(tmp_path, agent_config)
        update: dict[str, Any] = {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "Hello"},
        }
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        agent._buffers.append_response.assert_called_once_with("Hello")  # type: ignore[union-attr]

    def test_agent_message_chunk_posts_message(self, tmp_path: Path, agent_config: AgentConfig):
        agent = create_test_agent(tmp_path, agent_config)
        update: dict[str, Any] = {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "Hello"},
        }
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        agent.post_message.assert_called_once()  # type: ignore[union-attr]
        msg = agent.post_message.call_args[0][0]  # type: ignore[union-attr]
        assert isinstance(msg, messages.AgentUpdate)
        assert msg.text == "Hello"

    def test_agent_message_chunk_ignores_non_dict_content(
        self, tmp_path: Path, agent_config: AgentConfig
    ):
        agent = create_test_agent(tmp_path, agent_config)
        update: dict[str, Any] = {"sessionUpdate": "agent_message_chunk", "content": "string"}
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        agent._buffers.append_response.assert_not_called()  # type: ignore[union-attr]

    def test_agent_message_chunk_ignores_missing_content(
        self, tmp_path: Path, agent_config: AgentConfig
    ):
        agent = create_test_agent(tmp_path, agent_config)
        update: dict[str, Any] = {"sessionUpdate": "agent_message_chunk"}
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        agent._buffers.append_response.assert_not_called()  # type: ignore[union-attr]


class TestHandleSessionUpdateThought:
    def test_agent_thought_chunk_posts_thinking(self, tmp_path: Path, agent_config: AgentConfig):
        agent = create_test_agent(tmp_path, agent_config)
        update: dict[str, Any] = {
            "sessionUpdate": "agent_thought_chunk",
            "content": {"type": "reasoning", "text": "Let me think..."},
        }
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        msg = agent.post_message.call_args[0][0]  # type: ignore[union-attr]
        assert isinstance(msg, messages.Thinking)
        assert msg.text == "Let me think..."

    def test_agent_thought_chunk_ignores_non_dict(self, tmp_path: Path, agent_config: AgentConfig):
        agent = create_test_agent(tmp_path, agent_config)
        update: dict[str, Any] = {"sessionUpdate": "agent_thought_chunk", "content": None}
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        agent.post_message.assert_not_called()  # type: ignore[union-attr]


class TestHandleSessionUpdateToolCall:
    def test_tool_call_stores_in_agent(self, tmp_path: Path, agent_config: AgentConfig):
        agent = create_test_agent(tmp_path, agent_config)
        update: dict[str, Any] = {
            "sessionUpdate": "tool_call",
            "toolCallId": "call-123",
            "title": "Read file",
        }
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        assert "call-123" in agent.tool_calls
        assert agent.tool_calls["call-123"]["title"] == "Read file"

    def test_tool_call_posts_message(self, tmp_path: Path, agent_config: AgentConfig):
        agent = create_test_agent(tmp_path, agent_config)
        update: dict[str, Any] = {
            "sessionUpdate": "tool_call",
            "toolCallId": "call-123",
            "title": "Read file",
        }
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        msg = agent.post_message.call_args[0][0]  # type: ignore[union-attr]
        assert isinstance(msg, messages.ToolCall)


class TestHandleSessionUpdateToolCallUpdate:
    def test_tool_call_update_updates_existing(self, tmp_path: Path, agent_config: AgentConfig):
        agent = create_test_agent(tmp_path, agent_config)
        agent.tool_calls["call-123"] = {"toolCallId": "call-123", "title": "Read"}  # type: ignore
        update: dict[str, Any] = {
            "sessionUpdate": "tool_call_update",
            "toolCallId": "call-123",
            "status": "completed",
        }
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        assert agent.tool_calls["call-123"]["status"] == "completed"

    def test_tool_call_update_creates_if_missing(self, tmp_path: Path, agent_config: AgentConfig):
        agent = create_test_agent(tmp_path, agent_config)
        update: dict[str, Any] = {
            "sessionUpdate": "tool_call_update",
            "toolCallId": "new-call",
            "status": "running",
        }
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        assert "new-call" in agent.tool_calls
        assert agent.tool_calls["new-call"]["status"] == "running"

    def test_tool_call_update_posts_message(self, tmp_path: Path, agent_config: AgentConfig):
        agent = create_test_agent(tmp_path, agent_config)
        agent.tool_calls["call-123"] = {"toolCallId": "call-123"}  # type: ignore
        update: dict[str, Any] = {
            "sessionUpdate": "tool_call_update",
            "toolCallId": "call-123",
            "output": "result",
        }
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        msg = agent.post_message.call_args[0][0]  # type: ignore[union-attr]
        assert isinstance(msg, messages.ToolCallUpdate)


class TestHandleSessionUpdatePlan:
    def test_plan_posts_message(self, tmp_path: Path, agent_config: AgentConfig):
        agent = create_test_agent(tmp_path, agent_config)
        entries = [{"id": "1", "title": "Step 1"}, {"id": "2", "title": "Step 2"}]
        update: dict[str, Any] = {"sessionUpdate": "plan", "entries": entries}
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        msg = agent.post_message.call_args[0][0]  # type: ignore[union-attr]
        assert isinstance(msg, messages.Plan)
        assert len(msg.entries) == 2

    def test_plan_ignores_missing_entries(self, tmp_path: Path, agent_config: AgentConfig):
        agent = create_test_agent(tmp_path, agent_config)
        update: dict[str, Any] = {"sessionUpdate": "plan"}
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        agent.post_message.assert_not_called()  # type: ignore[union-attr]


class TestHandleSessionUpdateModes:
    def test_available_commands_posts_message(self, tmp_path: Path, agent_config: AgentConfig):
        agent = create_test_agent(tmp_path, agent_config)
        cmds = [{"id": "cmd1", "name": "/help"}]
        update: dict[str, Any] = {
            "sessionUpdate": "available_commands_update",
            "availableCommands": cmds,
        }
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        msg = agent.post_message.call_args[0][0]  # type: ignore[union-attr]
        assert isinstance(msg, messages.AvailableCommandsUpdate)

    def test_current_mode_posts_message(self, tmp_path: Path, agent_config: AgentConfig):
        agent = create_test_agent(tmp_path, agent_config)
        update: dict[str, Any] = {"sessionUpdate": "current_mode_update", "currentModeId": "code"}
        agent._rpc_session_update("session-1", cast("SessionUpdate", update))
        msg = agent.post_message.call_args[0][0]  # type: ignore[union-attr]
        assert isinstance(msg, messages.ModeUpdate)
        assert msg.current_mode == "code"


class TestHandleReadTextFile:
    def test_reads_file(self, tmp_path: Path, agent_config: AgentConfig):
        agent = Agent(tmp_path, agent_config, read_only=False)
        (tmp_path / "test.txt").write_text("line1\nline2\nline3")

        result = agent._rpc_read_text_file("s1", "test.txt")
        assert result["content"] == "line1\nline2\nline3"

    def test_reads_with_line_offset(self, tmp_path: Path, agent_config: AgentConfig):
        agent = Agent(tmp_path, agent_config, read_only=False)
        (tmp_path / "test.txt").write_text("line1\nline2\nline3")

        result = agent._rpc_read_text_file("s1", "test.txt", line=2)
        assert result["content"] == "line2\nline3"

    def test_reads_with_limit(self, tmp_path: Path, agent_config: AgentConfig):
        agent = Agent(tmp_path, agent_config, read_only=False)
        (tmp_path / "test.txt").write_text("line1\nline2\nline3\nline4")

        result = agent._rpc_read_text_file("s1", "test.txt", line=1, limit=2)
        assert result["content"] == "line1\nline2"

    def test_reads_with_offset_and_limit(self, tmp_path: Path, agent_config: AgentConfig):
        agent = Agent(tmp_path, agent_config, read_only=False)
        (tmp_path / "test.txt").write_text("a\nb\nc\nd\ne")

        result = agent._rpc_read_text_file("s1", "test.txt", line=2, limit=2)
        assert result["content"] == "b\nc"

    def test_returns_empty_on_missing_file(self, tmp_path: Path, agent_config: AgentConfig):
        agent = Agent(tmp_path, agent_config, read_only=False)
        result = agent._rpc_read_text_file("s1", "nonexistent.txt")
        assert result["content"] == ""

    def test_handles_subdirectory(self, tmp_path: Path, agent_config: AgentConfig):
        agent = Agent(tmp_path, agent_config, read_only=False)
        (tmp_path / "sub").mkdir()
        (tmp_path / "sub" / "file.txt").write_text("content")

        result = agent._rpc_read_text_file("s1", "sub/file.txt")
        assert result["content"] == "content"


class TestHandleWriteTextFile:
    def test_writes_file(self, tmp_path: Path, agent_config: AgentConfig):
        agent = Agent(tmp_path, agent_config, read_only=False)
        agent._rpc_write_text_file("s1", "output.txt", "hello world")
        assert (tmp_path / "output.txt").read_text() == "hello world"

    def test_creates_parent_directories(self, tmp_path: Path, agent_config: AgentConfig):
        agent = Agent(tmp_path, agent_config, read_only=False)
        agent._rpc_write_text_file("s1", "deep/nested/file.txt", "content")
        assert (tmp_path / "deep" / "nested" / "file.txt").read_text() == "content"

    def test_overwrites_existing_file(self, tmp_path: Path, agent_config: AgentConfig):
        agent = Agent(tmp_path, agent_config, read_only=False)
        (tmp_path / "existing.txt").write_text("old")
        agent._rpc_write_text_file("s1", "existing.txt", "new")
        assert (tmp_path / "existing.txt").read_text() == "new"


class TestHandleTerminalKill:
    def test_calls_terminals_kill(self, tmp_path: Path, agent_config: AgentConfig):
        agent = Agent(tmp_path, agent_config, read_only=False)
        agent._terminals = MagicMock()
        result = agent._rpc_terminal_kill("s1", "term-123")
        agent._terminals.kill.assert_called_once_with("term-123")
        assert result == {}


class TestHandleTerminalRelease:
    def test_calls_terminals_release(self, tmp_path: Path, agent_config: AgentConfig):
        agent = Agent(tmp_path, agent_config, read_only=False)
        agent._terminals = MagicMock()
        result = agent._rpc_terminal_release("s1", "term-123")
        agent._terminals.release.assert_called_once_with("term-123")
        assert result == {}
