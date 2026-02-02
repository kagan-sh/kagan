"""Integration tests for auto_approve logic in RPC permission handlers.

Tests hypothesis #2: AUTO mode ticket agents fail WRITE operations because
in agent.py's _rpc_request_permission, auto-approve triggers when
`_auto_approve=True` OR `_message_target is None`. If a message target is
set and _auto_approve=False, permissions block waiting for UI response (330s timeout).
"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Any, cast
from unittest.mock import MagicMock

import pytest

from kagan.acp import messages
from kagan.acp.agent import Agent

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.acp.protocol import PermissionOption, ToolCallUpdatePermissionRequest
    from kagan.config import AgentConfig

pytestmark = pytest.mark.integration


def make_options(kinds: list[str] | None = None) -> list[PermissionOption]:
    """Create permission options. Defaults to allow_once + reject_once."""
    kinds = kinds or ["allow_once", "reject_once"]
    return [
        {"kind": cast("Any", k), "name": k.replace("_", " ").title(), "optionId": f"{k}-id"}
        for k in kinds
    ]


def make_tool_call() -> ToolCallUpdatePermissionRequest:
    """Create a minimal tool call request for permission testing."""
    return cast(
        "ToolCallUpdatePermissionRequest",
        {"toolCallId": "tool-1", "title": "Write file", "kind": "edit"},
    )


def make_agent(
    tmp_path: Path,
    agent_config: AgentConfig,
    *,
    auto_approve: bool = False,
    has_target: bool = False,
) -> Agent:
    """Create agent with specific auto_approve and message_target settings."""
    agent = Agent(tmp_path, agent_config, read_only=False)
    agent._auto_approve = auto_approve
    agent._message_target = MagicMock() if has_target else None
    agent.post_message = MagicMock(return_value=True)  # type: ignore
    return agent


class TestAutoApproveBehavior:
    """Tests for auto-approve behavior (auto_approve=True OR no message target)."""

    @pytest.mark.parametrize(
        ("auto_approve", "has_target"),
        [
            (True, True),  # auto_approve=True takes precedence
            (True, False),  # auto_approve=True, no target
            (False, False),  # no target acts as fallback auto-approve
        ],
        ids=["auto_approve_with_target", "auto_approve_no_target", "no_target_fallback"],
    )
    async def test_returns_immediately_without_blocking(
        self, tmp_path: Path, agent_config: AgentConfig, auto_approve: bool, has_target: bool
    ):
        """Auto-approve paths return immediately (no 330s timeout)."""
        agent = make_agent(tmp_path, agent_config, auto_approve=auto_approve, has_target=has_target)
        result = await asyncio.wait_for(
            agent._rpc_request_permission("s1", make_options(), make_tool_call()),
            timeout=1.0,
        )
        assert result["outcome"].get("optionId") == "allow_once-id"

    @pytest.mark.parametrize(
        ("option_kinds", "expected_id"),
        [
            (["reject_once", "allow_always"], "allow_always-id"),  # Selects allow
            (["reject_once", "reject_always"], "reject_once-id"),  # Falls back to first
            (["reject_always", "allow_once"], "allow_once-id"),  # Selects allow
        ],
        ids=["selects_allow_over_reject", "fallback_to_first_if_no_allow", "allow_not_first"],
    )
    async def test_option_selection(
        self, tmp_path: Path, agent_config: AgentConfig, option_kinds: list[str], expected_id: str
    ):
        """Auto-approve selects 'allow' option or falls back to first."""
        agent = make_agent(tmp_path, agent_config, auto_approve=True)
        options = make_options(option_kinds)
        result = await agent._rpc_request_permission("s1", options, make_tool_call())
        assert result["outcome"].get("optionId") == expected_id

    async def test_does_not_post_request_permission_message(
        self, tmp_path: Path, agent_config: AgentConfig
    ):
        """When auto-approving, does not post RequestPermission to UI."""
        agent = make_agent(tmp_path, agent_config, auto_approve=True)
        await agent._rpc_request_permission("s1", make_options(), make_tool_call())
        for call in agent.post_message.call_args_list:  # type: ignore
            assert not isinstance(call[0][0], messages.RequestPermission)


class TestBlockingBehavior:
    """Tests for blocking when message_target set and _auto_approve=False."""

    async def test_waits_for_ui_response_via_future(
        self, tmp_path: Path, agent_config: AgentConfig
    ):
        """When has_target and auto_approve=False, waits for UI to resolve future."""
        agent = make_agent(tmp_path, agent_config, auto_approve=False, has_target=True)
        captured_future: asyncio.Future[messages.Answer] | None = None

        def capture(msg: Any) -> bool:
            nonlocal captured_future
            if isinstance(msg, messages.RequestPermission):
                captured_future = msg.result_future
            return True

        agent.post_message = MagicMock(side_effect=capture)  # type: ignore
        task = asyncio.create_task(
            agent._rpc_request_permission("s1", make_options(), make_tool_call())
        )
        await asyncio.sleep(0.01)

        assert captured_future is not None
        assert not captured_future.done()

        captured_future.set_result(messages.Answer("allow_once-id"))
        result = await asyncio.wait_for(task, timeout=1.0)
        assert result["outcome"].get("optionId") == "allow_once-id"

    async def test_posts_request_permission_message(
        self, tmp_path: Path, agent_config: AgentConfig
    ):
        """When blocking, posts RequestPermission message to UI."""
        agent = make_agent(tmp_path, agent_config, auto_approve=False, has_target=True)
        posted: list[Any] = []
        agent.post_message = MagicMock(side_effect=lambda m: posted.append(m) or True)  # type: ignore

        task = asyncio.create_task(
            agent._rpc_request_permission("s1", make_options(), make_tool_call())
        )
        await asyncio.sleep(0.01)
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task

        assert any(isinstance(m, messages.RequestPermission) for m in posted)

    async def test_does_not_complete_immediately(self, tmp_path: Path, agent_config: AgentConfig):
        """Blocking path should not complete without UI response."""
        agent = make_agent(tmp_path, agent_config, auto_approve=False, has_target=True)
        task = asyncio.create_task(
            agent._rpc_request_permission("s1", make_options(), make_tool_call())
        )
        await asyncio.sleep(0.05)
        assert not task.done(), "Should wait for UI, not auto-approve"
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task


class TestEdgeCases:
    """Edge cases in permission handling."""

    async def test_empty_options_returns_empty_option_id(
        self, tmp_path: Path, agent_config: AgentConfig
    ):
        """When no options provided, returns empty optionId."""
        agent = make_agent(tmp_path, agent_config, auto_approve=True)
        result = await agent._rpc_request_permission("s1", [], make_tool_call())
        assert result["outcome"].get("optionId") == ""

    async def test_tool_call_stored_before_permission_check(
        self, tmp_path: Path, agent_config: AgentConfig
    ):
        """Tool call is stored in agent.tool_calls."""
        agent = make_agent(tmp_path, agent_config, auto_approve=True)
        await agent._rpc_request_permission("s1", make_options(), make_tool_call())
        assert "tool-1" in agent.tool_calls
        assert agent.tool_calls["tool-1"]["title"] == "Write file"
