"""Tests for KaganAgent sensitive-command detection."""

from __future__ import annotations

from pathlib import Path
from typing import Any, cast

import pytest
from acp.schema import AllowedOutcome, PermissionOption, ToolCallUpdate
from tests.helpers.mocks import create_test_agent_config

from kagan.core.acp import messages
from kagan.core.acp.kagan_agent import KaganAgent


def _build_agent() -> KaganAgent:
    return KaganAgent(Path("."), create_test_agent_config())


def _build_options() -> list[PermissionOption]:
    return [
        PermissionOption(kind="allow_once", name="Allow once", optionId="allow-once"),
        PermissionOption(kind="reject_once", name="Reject once", optionId="reject-once"),
    ]


def _build_tool_call_update() -> ToolCallUpdate:
    return ToolCallUpdate(toolCallId="tool-1", title="Run command")


class _ImmediateAnswerTarget:
    def __init__(self, answer_id: str) -> None:
        self._answer_id = answer_id

    def post_message(self, message) -> bool:
        if isinstance(message, messages.RequestPermission):
            message.result_future.set_result(messages.Answer(self._answer_id))
        return True


def test_allows_commit_gpgsign_config_flag() -> None:
    agent = _build_agent()
    command = 'git -c commit.gpgsign=false commit -m "fix: test"'
    assert not agent._command_mentions_sensitive(command, None)


def test_blocks_sensitive_env_reference() -> None:
    agent = _build_agent()
    assert agent._command_mentions_sensitive("cat .env", None)


def test_blocks_sensitive_extension_in_assignment_arg() -> None:
    agent = _build_agent()
    command = "curl --cert=certs/prod.pem https://example.com"
    assert agent._command_mentions_sensitive(command, None)


def test_blocks_sensitive_ssh_key_path() -> None:
    agent = _build_agent()
    assert agent._command_mentions_sensitive("cat ~/.ssh/id_rsa", None)


@pytest.mark.parametrize("field_name", ["_stop_requested", "_prompt_completed"])
def test_sigterm_is_ignored_after_shutdown_signal(field_name: str) -> None:
    agent = _build_agent()
    setattr(agent, field_name, True)
    assert agent._should_ignore_exit_code(-15)


def test_non_sigterm_exit_is_not_ignored() -> None:
    agent = _build_agent()
    agent._stop_requested = True
    assert not agent._should_ignore_exit_code(1)


@pytest.mark.asyncio()
async def test_request_permission_auto_approves_when_enabled() -> None:
    agent = _build_agent()
    agent.set_auto_approve(True)

    response = await agent.request_permission(
        options=_build_options(),
        session_id="session-1",
        tool_call=_build_tool_call_update(),
    )

    assert isinstance(response.outcome, AllowedOutcome)
    assert response.outcome.option_id == "allow-once"


@pytest.mark.asyncio()
async def test_request_permission_auto_approves_without_message_target() -> None:
    agent = _build_agent()

    response = await agent.request_permission(
        options=_build_options(),
        session_id="session-1",
        tool_call=_build_tool_call_update(),
    )

    assert isinstance(response.outcome, AllowedOutcome)
    assert response.outcome.option_id == "allow-once"


@pytest.mark.asyncio()
async def test_request_permission_uses_ui_answer_when_target_exists() -> None:
    agent = _build_agent()
    agent.set_message_target(cast("Any", _ImmediateAnswerTarget("reject-once")))

    response = await agent.request_permission(
        options=_build_options(),
        session_id="session-1",
        tool_call=_build_tool_call_update(),
    )

    assert isinstance(response.outcome, AllowedOutcome)
    assert response.outcome.option_id == "reject-once"
