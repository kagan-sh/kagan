"""Integration tests for Agent read_only flag and capability negotiation.

Tests verify:
1. Agent read_only flag initialization and propagation
2. ACP capability negotiation based on read_only setting
3. Write/terminal operation guards based on read_only flag
4. Scheduler creates agents with correct capabilities
"""

from __future__ import annotations

from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from kagan.acp.agent import Agent
from kagan.agents.scheduler import Scheduler
from kagan.database.models import Ticket, TicketStatus, TicketType

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.config import AgentConfig, KaganConfig
    from kagan.database.manager import StateManager

pytestmark = pytest.mark.integration


class TestAgentReadOnlyFlag:
    """Test Agent class properly handles read_only flag."""

    @pytest.mark.parametrize(
        ("read_only", "expected"),
        [(None, False), (True, True), (False, False)],
        ids=["default", "explicit-true", "explicit-false"],
    )
    def test_agent_read_only_flag(
        self, read_only, expected, tmp_path: Path, agent_config: AgentConfig
    ):
        """Agent read_only flag should be set correctly based on input."""
        kwargs = {"read_only": read_only} if read_only is not None else {}
        agent = Agent(tmp_path, agent_config, **kwargs)
        assert agent._read_only is expected


class TestCapabilityNegotiation:
    """Test read_only flag propagates to ACP capability negotiation."""

    @pytest.mark.parametrize(
        ("read_only", "expect_write", "expect_terminal"),
        [(False, True, True), (True, False, False)],
        ids=["worker-agent", "review-agent"],
    )
    async def test_capability_negotiation(
        self,
        read_only: bool,
        expect_write: bool,
        expect_terminal: bool,
        tmp_path: Path,
        agent_config: AgentConfig,
        mocker,
    ):
        """Agent capabilities should match read_only setting."""
        agent = Agent(tmp_path, agent_config, read_only=read_only)

        # Track the call made by _client.call
        captured_calls: list[tuple[str, dict]] = []

        def capturing_call(method: str, **params):
            captured_calls.append((method, params))
            # Return a mock PendingCall
            mock_call = MagicMock()
            mock_call.wait = AsyncMock(return_value={"agentCapabilities": {}})
            return mock_call

        mocker.patch.object(agent._client, "call", side_effect=capturing_call)

        await agent._acp_initialize()

        assert len(captured_calls) == 1
        method, params = captured_calls[0]
        assert method == "initialize"
        client_caps = params["clientCapabilities"]

        assert client_caps["fs"]["readTextFile"] is True
        if expect_write:
            assert client_caps["fs"]["writeTextFile"] is True
        else:
            assert "writeTextFile" not in client_caps["fs"]
        assert client_caps["terminal"] is expect_terminal

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
        """Empty agent capabilities response is accepted without validation."""
        agent = Agent(tmp_path, agent_config, read_only=False)
        agent._process = mock_process

        response = {"protocolVersion": "1", "agentCapabilities": {}}

        mock_call = MagicMock()
        mock_call.wait = AsyncMock(return_value=response)

        with patch.object(agent._client, "call", return_value=mock_call):
            await agent._acp_initialize()

        assert agent.agent_capabilities == {}


class TestWriteOperationBlocking:
    """Test write operations are blocked/allowed based on read_only flag."""

    @pytest.mark.parametrize(
        ("read_only", "should_block"),
        [(True, True), (False, False)],
        ids=["read-only-blocked", "write-allowed"],
    )
    def test_write_file_permission(
        self, read_only: bool, should_block: bool, tmp_path: Path, agent_config: AgentConfig
    ):
        """Write operations should respect read_only flag."""
        agent = Agent(tmp_path, agent_config, read_only=read_only)

        if should_block:
            with pytest.raises(ValueError, match="not permitted in read-only mode"):
                agent._rpc_write_text_file("session-1", "test.txt", "content")
            # Verify file was not created
            assert not (tmp_path / "test.txt").exists()
        else:
            agent._rpc_write_text_file("session-1", "test.txt", "content")
            assert (tmp_path / "test.txt").read_text() == "content"

    @pytest.mark.parametrize(
        ("read_only", "should_block"),
        [(True, True), (False, False)],
        ids=["read-only-blocked", "terminal-allowed"],
    )
    async def test_terminal_permission(
        self, read_only: bool, should_block: bool, tmp_path: Path, agent_config: AgentConfig, mocker
    ):
        """Terminal operations should respect read_only flag."""
        agent = Agent(tmp_path, agent_config, read_only=read_only)

        if should_block:
            with pytest.raises(ValueError, match="not permitted in read-only mode"):
                await agent._rpc_terminal_create("echo test")
        else:
            mock_create = mocker.patch.object(
                agent._terminals, "create", new=AsyncMock(return_value=("term-1", "echo test"))
            )
            result = await agent._rpc_terminal_create("echo test")
            mock_create.assert_called_once()
            assert result["terminalId"] == "term-1"


class TestSchedulerAgentCreation:
    """Test Scheduler creates agents with correct read_only flag."""

    @pytest.mark.parametrize(
        ("is_review", "expected_read_only", "response_text"),
        [(False, False, "<complete/>"), (True, True, '<approve summary="Looks good"/>')],
        ids=["worker-agent", "review-agent"],
    )
    async def test_scheduler_agent_read_only(
        self,
        is_review: bool,
        expected_read_only: bool,
        response_text: str,
        state_manager: StateManager,
        mock_worktree_manager,
        config: KaganConfig,
        mocker,
    ):
        """Scheduler should create agents with correct read_only flag."""
        scheduler = Scheduler(
            state_manager=state_manager,
            worktree_manager=mock_worktree_manager,
            config=config,
        )

        ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Test ticket",
                ticket_type=TicketType.AUTO,
                status=TicketStatus.IN_PROGRESS,
            )
        )

        agent_instances = []

        def capture_agent(*args, **kwargs):
            instance = MagicMock()
            instance._read_only = kwargs.get("read_only", False)
            instance.set_auto_approve = MagicMock()
            instance.start = MagicMock()
            instance.wait_ready = AsyncMock()
            instance.send_prompt = AsyncMock()
            instance.get_response_text = MagicMock(return_value=response_text)
            instance.stop = AsyncMock()
            agent_instances.append({"args": args, "kwargs": kwargs, "instance": instance})
            return instance

        mocker.patch("kagan.agents.scheduler.Agent", side_effect=capture_agent)

        full_ticket = await state_manager.get_ticket(ticket.id)
        assert full_ticket is not None

        wt_path = await mock_worktree_manager.get_path(ticket.id)

        if is_review:
            await scheduler._run_review(full_ticket, wt_path)
        else:
            agent_config = scheduler._get_agent_config(full_ticket)
            await scheduler._run_iteration(full_ticket, wt_path, agent_config, 1, 3)

        assert len(agent_instances) == 1
        assert agent_instances[0]["kwargs"].get("read_only", False) is expected_read_only
