"""Tests for ACP RPC handlers."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, cast

import pytest

from kagan.acp import messages
from kagan.acp.rpc import (
    handle_read_text_file,
    handle_session_update,
    handle_terminal_kill,
    handle_terminal_release,
    handle_write_text_file,
)

if TYPE_CHECKING:
    from pathlib import Path
    from unittest.mock import MagicMock

    from pytest_mock import MockerFixture

    from kagan.acp.protocol import SessionUpdate

pytestmark = pytest.mark.integration


@pytest.fixture
def mock_agent(tmp_path: Path, mocker: MockerFixture):
    """Create a mock Agent with required attributes."""
    agent = mocker.MagicMock(
        spec=[
            "_buffers",
            "_terminals",
            "_read_only",
            "tool_calls",
            "post_message",
            "project_root",
        ]
    )
    agent._buffers = mocker.MagicMock()
    agent._buffers.append_response = mocker.MagicMock()
    agent._terminals = mocker.MagicMock()
    agent._read_only = False  # Allow writes by default in tests
    agent.tool_calls = {}
    agent.post_message = mocker.MagicMock()
    agent.project_root = tmp_path
    return agent


class TestHandleSessionUpdateAgentMessage:
    def test_agent_message_chunk_appends_to_buffer(self, mock_agent: MagicMock):
        update: dict[str, Any] = {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "Hello"},
        }
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        mock_agent._buffers.append_response.assert_called_once_with("Hello")

    def test_agent_message_chunk_posts_message(self, mock_agent: MagicMock):
        update: dict[str, Any] = {
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": "Hello"},
        }
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        mock_agent.post_message.assert_called_once()
        msg = mock_agent.post_message.call_args[0][0]
        assert isinstance(msg, messages.AgentUpdate)
        assert msg.text == "Hello"

    def test_agent_message_chunk_ignores_non_dict_content(self, mock_agent: MagicMock):
        update: dict[str, Any] = {"sessionUpdate": "agent_message_chunk", "content": "string"}
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        mock_agent._buffers.append_response.assert_not_called()

    def test_agent_message_chunk_ignores_missing_content(self, mock_agent: MagicMock):
        update: dict[str, Any] = {"sessionUpdate": "agent_message_chunk"}
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        mock_agent._buffers.append_response.assert_not_called()


class TestHandleSessionUpdateThought:
    def test_agent_thought_chunk_posts_thinking(self, mock_agent: MagicMock):
        update: dict[str, Any] = {
            "sessionUpdate": "agent_thought_chunk",
            "content": {"type": "reasoning", "text": "Let me think..."},
        }
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        msg = mock_agent.post_message.call_args[0][0]
        assert isinstance(msg, messages.Thinking)
        assert msg.text == "Let me think..."

    def test_agent_thought_chunk_ignores_non_dict(self, mock_agent: MagicMock):
        update: dict[str, Any] = {"sessionUpdate": "agent_thought_chunk", "content": None}
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        mock_agent.post_message.assert_not_called()


class TestHandleSessionUpdateToolCall:
    def test_tool_call_stores_in_agent(self, mock_agent: MagicMock):
        update: dict[str, Any] = {
            "sessionUpdate": "tool_call",
            "toolCallId": "call-123",
            "title": "Read file",
        }
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        assert "call-123" in mock_agent.tool_calls
        assert mock_agent.tool_calls["call-123"]["title"] == "Read file"

    def test_tool_call_posts_message(self, mock_agent: MagicMock):
        update: dict[str, Any] = {
            "sessionUpdate": "tool_call",
            "toolCallId": "call-123",
            "title": "Read file",
        }
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        msg = mock_agent.post_message.call_args[0][0]
        assert isinstance(msg, messages.ToolCall)


class TestHandleSessionUpdateToolCallUpdate:
    def test_tool_call_update_updates_existing(self, mock_agent: MagicMock):
        mock_agent.tool_calls["call-123"] = {"toolCallId": "call-123", "title": "Read"}
        update: dict[str, Any] = {
            "sessionUpdate": "tool_call_update",
            "toolCallId": "call-123",
            "status": "completed",
        }
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        assert mock_agent.tool_calls["call-123"]["status"] == "completed"

    def test_tool_call_update_creates_if_missing(self, mock_agent: MagicMock):
        update: dict[str, Any] = {
            "sessionUpdate": "tool_call_update",
            "toolCallId": "new-call",
            "status": "running",
        }
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        assert "new-call" in mock_agent.tool_calls
        assert mock_agent.tool_calls["new-call"]["status"] == "running"

    def test_tool_call_update_posts_message(self, mock_agent: MagicMock):
        mock_agent.tool_calls["call-123"] = {"toolCallId": "call-123"}
        update: dict[str, Any] = {
            "sessionUpdate": "tool_call_update",
            "toolCallId": "call-123",
            "output": "result",
        }
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        msg = mock_agent.post_message.call_args[0][0]
        assert isinstance(msg, messages.ToolCallUpdate)


class TestHandleSessionUpdatePlan:
    def test_plan_posts_message(self, mock_agent: MagicMock):
        entries = [{"id": "1", "title": "Step 1"}, {"id": "2", "title": "Step 2"}]
        update: dict[str, Any] = {"sessionUpdate": "plan", "entries": entries}
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        msg = mock_agent.post_message.call_args[0][0]
        assert isinstance(msg, messages.Plan)
        assert len(msg.entries) == 2

    def test_plan_ignores_missing_entries(self, mock_agent: MagicMock):
        update: dict[str, Any] = {"sessionUpdate": "plan"}
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        mock_agent.post_message.assert_not_called()


class TestHandleSessionUpdateModes:
    def test_available_commands_posts_message(self, mock_agent: MagicMock):
        cmds = [{"id": "cmd1", "name": "/help"}]
        update: dict[str, Any] = {
            "sessionUpdate": "available_commands_update",
            "availableCommands": cmds,
        }
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        msg = mock_agent.post_message.call_args[0][0]
        assert isinstance(msg, messages.AvailableCommandsUpdate)

    def test_current_mode_posts_message(self, mock_agent: MagicMock):
        update: dict[str, Any] = {"sessionUpdate": "current_mode_update", "currentModeId": "code"}
        handle_session_update(mock_agent, "session-1", cast("SessionUpdate", update))
        msg = mock_agent.post_message.call_args[0][0]
        assert isinstance(msg, messages.ModeUpdate)
        assert msg.current_mode == "code"


class TestHandleReadTextFile:
    def test_reads_file(self, mock_agent: MagicMock, tmp_path: Path):
        mock_agent.project_root = tmp_path
        (tmp_path / "test.txt").write_text("line1\nline2\nline3")

        result = handle_read_text_file(mock_agent, "s1", "test.txt")
        assert result["content"] == "line1\nline2\nline3"

    def test_reads_with_line_offset(self, mock_agent: MagicMock, tmp_path: Path):
        mock_agent.project_root = tmp_path
        (tmp_path / "test.txt").write_text("line1\nline2\nline3")

        result = handle_read_text_file(mock_agent, "s1", "test.txt", line=2)
        assert result["content"] == "line2\nline3"

    def test_reads_with_limit(self, mock_agent: MagicMock, tmp_path: Path):
        mock_agent.project_root = tmp_path
        (tmp_path / "test.txt").write_text("line1\nline2\nline3\nline4")

        result = handle_read_text_file(mock_agent, "s1", "test.txt", line=1, limit=2)
        assert result["content"] == "line1\nline2"

    def test_reads_with_offset_and_limit(self, mock_agent: MagicMock, tmp_path: Path):
        mock_agent.project_root = tmp_path
        (tmp_path / "test.txt").write_text("a\nb\nc\nd\ne")

        result = handle_read_text_file(mock_agent, "s1", "test.txt", line=2, limit=2)
        assert result["content"] == "b\nc"

    def test_returns_empty_on_missing_file(self, mock_agent: MagicMock, tmp_path: Path):
        mock_agent.project_root = tmp_path
        result = handle_read_text_file(mock_agent, "s1", "nonexistent.txt")
        assert result["content"] == ""

    def test_handles_subdirectory(self, mock_agent: MagicMock, tmp_path: Path):
        mock_agent.project_root = tmp_path
        (tmp_path / "sub").mkdir()
        (tmp_path / "sub" / "file.txt").write_text("content")

        result = handle_read_text_file(mock_agent, "s1", "sub/file.txt")
        assert result["content"] == "content"


class TestHandleWriteTextFile:
    def test_writes_file(self, mock_agent: MagicMock, tmp_path: Path):
        mock_agent.project_root = tmp_path
        handle_write_text_file(mock_agent, "s1", "output.txt", "hello world")
        assert (tmp_path / "output.txt").read_text() == "hello world"

    def test_creates_parent_directories(self, mock_agent: MagicMock, tmp_path: Path):
        mock_agent.project_root = tmp_path
        handle_write_text_file(mock_agent, "s1", "deep/nested/file.txt", "content")
        assert (tmp_path / "deep" / "nested" / "file.txt").read_text() == "content"

    def test_overwrites_existing_file(self, mock_agent: MagicMock, tmp_path: Path):
        mock_agent.project_root = tmp_path
        (tmp_path / "existing.txt").write_text("old")
        handle_write_text_file(mock_agent, "s1", "existing.txt", "new")
        assert (tmp_path / "existing.txt").read_text() == "new"


class TestHandleTerminalKill:
    def test_calls_terminals_kill(self, mock_agent: MagicMock):
        result = handle_terminal_kill(mock_agent, "s1", "term-123")
        mock_agent._terminals.kill.assert_called_once_with("term-123")
        assert result == {}


class TestHandleTerminalRelease:
    def test_calls_terminals_release(self, mock_agent: MagicMock):
        result = handle_terminal_release(mock_agent, "s1", "term-123")
        mock_agent._terminals.release.assert_called_once_with("term-123")
        assert result == {}
