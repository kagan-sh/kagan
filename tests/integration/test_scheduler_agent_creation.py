"""Integration tests for scheduler agent creation in AUTO mode.

Verifies worker agents have write permissions, review agents are read-only,
auto_approve is configured correctly, and startup failures are reported.
"""

from __future__ import annotations

import asyncio
from pathlib import Path
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock

import pytest

from kagan.agents.scheduler import Scheduler
from kagan.config import AgentConfig, GeneralConfig, KaganConfig
from kagan.database.models import Ticket, TicketStatus, TicketType

if TYPE_CHECKING:
    from kagan.database.manager import StateManager

pytestmark = pytest.mark.integration


def _make_mock_agent(response: str = "<complete/>") -> MagicMock:
    """Create a mock agent with standard methods."""
    mock = MagicMock()
    mock.set_auto_approve = MagicMock()
    mock.start = MagicMock()
    mock.wait_ready = AsyncMock()
    mock.send_prompt = AsyncMock()
    mock.get_response_text = MagicMock(return_value=response)
    mock.stop = AsyncMock()
    return mock


def _make_config(auto_approve: bool = False) -> KaganConfig:
    """Helper to create test config."""
    return KaganConfig(
        general=GeneralConfig(
            auto_start=True,
            auto_approve=auto_approve,
            max_iterations=3,
            iteration_delay_seconds=0.01,
            default_worker_agent="test",
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


class TestAgentCreation:
    """Tests for agent creation with correct read_only and auto_approve settings."""

    @pytest.mark.parametrize(
        ("agent_type", "expected_read_only"),
        [
            ("worker", False),
            ("review", True),
        ],
        ids=["worker_has_write_permission", "review_is_read_only"],
    )
    async def test_agent_read_only_setting(
        self,
        agent_type: str,
        expected_read_only: bool,
        scheduler: Scheduler,
        state_manager: StateManager,
        mocker,
    ):
        """Agent created with correct read_only setting based on type."""
        ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Test", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS
            )
        )

        created: list[dict] = []

        def track(*args, **kwargs):
            created.append({"args": args, "kwargs": kwargs})
            response = '<approve summary="LGTM"/>' if agent_type == "review" else "<complete/>"
            return _make_mock_agent(response)

        mocker.patch("kagan.agents.scheduler.Agent", side_effect=track)

        if agent_type == "worker":
            scheduler.start()
            await scheduler.spawn_for_ticket(ticket)
            for _ in range(30):
                await asyncio.sleep(0.1)
                if len(created) >= 1:
                    break
            await scheduler.stop()
        else:
            await scheduler._run_review(ticket, Path("/tmp/wt"))

        assert len(created) >= 1, f"{agent_type} agent not created"
        idx = 0 if agent_type == "worker" else -1
        assert created[idx]["kwargs"].get("read_only", False) is expected_read_only

    @pytest.mark.parametrize(
        ("agent_type", "config_auto_approve", "expected_auto_approve"),
        [
            ("worker", True, True),
            ("review", False, True),
        ],
        ids=["worker_uses_config_auto_approve", "review_always_auto_approves"],
    )
    async def test_agent_auto_approve_setting(
        self,
        agent_type: str,
        config_auto_approve: bool,
        expected_auto_approve: bool,
        state_manager: StateManager,
        mock_worktree_manager,
        mocker,
    ):
        """Agent auto_approve set correctly based on type and config."""
        config = _make_config(auto_approve=config_auto_approve)
        scheduler = Scheduler(state_manager, mock_worktree_manager, config)

        ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Test", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS
            )
        )

        response = '<approve summary="OK"/>' if agent_type == "review" else "<complete/>"
        mock_agent = _make_mock_agent(response)
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        if agent_type == "worker":
            scheduler.start()
            await scheduler.spawn_for_ticket(ticket)
            for _ in range(20):
                await asyncio.sleep(0.05)
                if mock_agent.set_auto_approve.called:
                    break
            await scheduler.stop()
        else:
            await scheduler._run_review(ticket, Path("/tmp/wt"))

        mock_agent.set_auto_approve.assert_called_with(expected_auto_approve)


class TestAgentStartupFailure:
    """Tests for agent startup failure handling."""

    @pytest.mark.parametrize(
        ("agent_type", "expected_signal_or_passed", "error_substring"),
        [
            ("worker", "BLOCKED", "failed to start"),
            ("review", False, "timed out"),
        ],
        ids=["worker_timeout_returns_blocked", "review_timeout_returns_failure"],
    )
    async def test_agent_timeout_handling(
        self,
        agent_type: str,
        expected_signal_or_passed: str | bool,
        error_substring: str,
        scheduler: Scheduler,
        state_manager: StateManager,
        mocker,
    ):
        """Agent timeout returns appropriate failure signal/result."""
        from kagan.agents.signals import Signal

        ticket = await state_manager.create_ticket(
            Ticket.create(
                title="Timeout", ticket_type=TicketType.AUTO, status=TicketStatus.IN_PROGRESS
            )
        )

        mock_agent = _make_mock_agent()
        mock_agent.wait_ready = AsyncMock(side_effect=TimeoutError("Timeout"))
        mocker.patch("kagan.agents.scheduler.Agent", return_value=mock_agent)

        if agent_type == "worker":
            full = await state_manager.get_ticket(ticket.id)
            assert full is not None
            cfg = scheduler._get_agent_config(full)
            signal = await scheduler._run_iteration(full, Path("/tmp/wt"), cfg, 1, 3)
            assert signal.signal == Signal.BLOCKED
            assert error_substring in signal.reason.lower()
        else:
            passed, summary = await scheduler._run_review(ticket, Path("/tmp/wt"))
            assert passed is expected_signal_or_passed
            assert error_substring in summary.lower()
