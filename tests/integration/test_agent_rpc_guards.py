"""Integration tests for Agent RPC guards in read_only mode."""

from __future__ import annotations

from unittest.mock import MagicMock

import pytest

from kagan.acp import rpc

pytestmark = pytest.mark.integration


class TestReadOnlyGuards:
    """Tests for RPC handlers rejecting operations in read_only mode."""

    def test_write_text_file_blocked_in_read_only_mode(self):
        """fs/write_text_file raises when agent is read_only."""
        agent = MagicMock()
        agent._read_only = True

        with pytest.raises(ValueError, match="not permitted in read-only mode"):
            rpc.handle_write_text_file(agent, "session-1", "test.txt", "content")

    def test_write_text_file_allowed_in_normal_mode(self, tmp_path):
        """fs/write_text_file works when agent is not read_only."""
        agent = MagicMock()
        agent._read_only = False
        agent.project_root = tmp_path

        rpc.handle_write_text_file(agent, "session-1", "test.txt", "content")

        assert (tmp_path / "test.txt").read_text() == "content"

    async def test_terminal_create_blocked_in_read_only_mode(self):
        """terminal/create raises when agent is read_only."""
        agent = MagicMock()
        agent._read_only = True

        with pytest.raises(ValueError, match="not permitted in read-only mode"):
            await rpc.handle_terminal_create(agent, "echo hello")

    def test_read_text_file_allowed_in_read_only_mode(self, tmp_path):
        """fs/read_text_file works when agent is read_only."""
        agent = MagicMock()
        agent._read_only = True
        agent.project_root = tmp_path

        # Create a test file
        test_file = tmp_path / "test.txt"
        test_file.write_text("hello world")

        result = rpc.handle_read_text_file(agent, "session-1", "test.txt")

        assert result["content"] == "hello world"
