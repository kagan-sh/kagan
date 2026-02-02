"""Integration tests for ACP write operation flow in AUTO mode.

Tests the complete flow: RPC receive -> permission check -> file write -> response.
"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Any

import pytest

from kagan.acp.agent import Agent
from tests.helpers.mocks import create_test_agent_config

if TYPE_CHECKING:
    from pathlib import Path

pytestmark = pytest.mark.integration


class TestWriteTextFileFlow:
    """Test fs/write_text_file RPC handling end-to-end."""

    @pytest.mark.parametrize(
        ("read_only", "path", "content", "expect_success"),
        [
            (False, "test_output.txt", "Hello from AUTO mode agent!", True),
            (False, "deep/nested/path/file.txt", "Nested content", True),
            (True, "should_not_exist.txt", "This should fail", False),
        ],
        ids=["basic-write", "nested-dirs", "read-only-blocked"],
    )
    async def test_write_file_via_rpc(
        self,
        read_only: bool,
        path: str,
        content: str,
        expect_success: bool,
        tmp_path: Path,
    ):
        """Test fs/write_text_file RPC with various configurations."""
        agent = Agent(
            project_root=tmp_path,
            agent_config=create_test_agent_config(),
            read_only=read_only,
        )
        agent.set_auto_approve(True)

        rpc_request: dict[str, Any] = {
            "jsonrpc": "2.0",
            "method": "fs/write_text_file",
            "params": {
                "sessionId": "test-session",
                "path": path,
                "content": content,
            },
            "id": 1,
        }

        response = await agent.server.call(rpc_request)
        assert isinstance(response, dict)

        if expect_success:
            assert "error" not in response, f"Unexpected error: {response.get('error')}"
            output_file = tmp_path / path
            assert output_file.exists(), "File was not written to disk"
            assert output_file.read_text() == content
        else:
            assert "error" in response
            error = response["error"]
            assert isinstance(error, dict)
            message = error.get("message", "")
            assert isinstance(message, str)
            assert "read-only" in message.lower()
            assert not (tmp_path / path).exists()


class TestReadTextFileFlow:
    """Test fs/read_text_file RPC handling end-to-end."""

    async def test_read_file_via_rpc_call(self, tmp_path: Path):
        """Full flow: JSON-RPC fs/read_text_file -> file content returned."""
        (tmp_path / "existing.txt").write_text("Existing file content")

        agent = Agent(
            project_root=tmp_path,
            agent_config=create_test_agent_config(),
            read_only=True,
        )

        rpc_request: dict[str, Any] = {
            "jsonrpc": "2.0",
            "method": "fs/read_text_file",
            "params": {
                "sessionId": "test-session",
                "path": "existing.txt",
            },
            "id": 4,
        }

        response = await agent.server.call(rpc_request)

        assert isinstance(response, dict)
        assert "error" not in response
        result = response["result"]
        assert isinstance(result, dict)
        assert result["content"] == "Existing file content"

    async def test_read_file_with_line_and_limit(self, tmp_path: Path):
        """RPC handles line offset and limit parameters."""
        (tmp_path / "multiline.txt").write_text("line1\nline2\nline3\nline4\nline5")

        agent = Agent(
            project_root=tmp_path,
            agent_config=create_test_agent_config(),
        )

        rpc_request: dict[str, Any] = {
            "jsonrpc": "2.0",
            "method": "fs/read_text_file",
            "params": {
                "sessionId": "test-session",
                "path": "multiline.txt",
                "line": 2,
                "limit": 2,
            },
            "id": 5,
        }

        response = await agent.server.call(rpc_request)
        assert isinstance(response, dict)
        assert "error" not in response
        result = response["result"]
        assert isinstance(result, dict)
        assert result["content"] == "line2\nline3"


class TestTerminalCreateFlow:
    """Test terminal/create RPC handling end-to-end."""

    @pytest.mark.parametrize(
        ("read_only", "expect_success"),
        [
            (True, False),  # blocked in read-only mode
            (False, True),  # allowed in write mode
        ],
        ids=["read-only-blocked", "write-allowed"],
    )
    async def test_terminal_create_permission(
        self, read_only: bool, expect_success: bool, tmp_path: Path
    ):
        """terminal/create should respect read_only flag."""
        agent = Agent(
            project_root=tmp_path,
            agent_config=create_test_agent_config(),
            read_only=read_only,
        )
        agent.set_auto_approve(True)

        rpc_request: dict[str, Any] = {
            "jsonrpc": "2.0",
            "method": "terminal/create",
            "params": {
                "command": "echo",
                "args": ["hello"],
            },
            "id": 6,
        }

        response = await agent.server.call(rpc_request)
        assert isinstance(response, dict)

        if expect_success:
            assert "error" not in response, f"Unexpected error: {response.get('error')}"
            result = response.get("result", {})
            assert isinstance(result, dict)
            assert "terminalId" in result
        else:
            assert "error" in response
            error = response["error"]
            assert isinstance(error, dict)
            message = error.get("message", "")
            assert isinstance(message, str)
            assert "read-only" in message.lower()


class TestAutoApproveWithWriteFlow:
    """Test that auto_approve=True allows writes without blocking."""

    async def test_write_completes_immediately_with_auto_approve(self, tmp_path: Path):
        """Writes should not block waiting for permission when auto_approve=True."""
        agent = Agent(
            project_root=tmp_path,
            agent_config=create_test_agent_config(),
            read_only=False,
        )
        agent.set_auto_approve(True)

        rpc_request: dict[str, Any] = {
            "jsonrpc": "2.0",
            "method": "fs/write_text_file",
            "params": {
                "sessionId": "test-session",
                "path": "quick_write.txt",
                "content": "Should complete quickly",
            },
            "id": 8,
        }

        response = await asyncio.wait_for(agent.server.call(rpc_request), timeout=1.0)

        assert isinstance(response, dict)
        assert "error" not in response
        assert (tmp_path / "quick_write.txt").exists()


class TestAgentInitializationWithWriteMode:
    """Test Agent initialization correctly sets up write capabilities."""

    @pytest.mark.parametrize(
        ("read_only", "expected"),
        [
            (False, False),  # AUTO mode
            (True, True),  # REVIEW mode
        ],
        ids=["auto-mode", "review-mode"],
    )
    def test_agent_read_only_initialization(self, read_only: bool, expected: bool, tmp_path: Path):
        """Agent read_only should be set correctly based on mode."""
        agent = Agent(
            project_root=tmp_path,
            agent_config=create_test_agent_config(),
            read_only=read_only,
        )
        assert agent._read_only == expected

    def test_auto_approve_can_be_set(self, tmp_path: Path):
        """auto_approve can be enabled after agent creation."""
        agent = Agent(
            project_root=tmp_path,
            agent_config=create_test_agent_config(),
        )
        assert agent._auto_approve == False  # noqa: E712

        agent.set_auto_approve(True)
        assert agent._auto_approve == True  # noqa: E712
