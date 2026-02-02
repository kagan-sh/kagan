"""Unit tests for Agent error handling paths."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from kagan.acp import messages
from kagan.acp.agent import Agent
from kagan.acp.jsonrpc import RPCError
from tests.helpers.mocks import AgentTestHarness

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.config import AgentConfig

pytestmark = pytest.mark.unit


@pytest.fixture
def harness(tmp_path: Path, agent_config: AgentConfig) -> AgentTestHarness:
    return AgentTestHarness(tmp_path, agent_config)


# === Startup Failures ===
@pytest.mark.parametrize(
    "error,expected",
    [
        (OSError("Command not found"), "Command not found"),
        (PermissionError("Permission denied"), "Permission denied"),
    ],
)
async def test_subprocess_creation_errors(
    harness: AgentTestHarness, error: Exception, expected: str
):
    with patch("asyncio.create_subprocess_shell", side_effect=error):
        await harness.agent._run_agent()
    harness.assert_posted_fail(expected)


async def test_no_command_posts_fail(harness: AgentTestHarness):
    with patch.object(type(harness.agent), "command", property(lambda self: None)):
        await harness.agent._run_agent()
    harness.assert_posted_fail("No run command")


# === Read Loop Crash Handling ===
@pytest.mark.parametrize(
    "bad_lines",
    [
        [b"not valid json\n", b"{malformed\n"],
        [b"\xff\xfe invalid utf8\n"],
        [b"\n", b"   \t  \n"],
    ],
)
async def test_read_loop_handles_bad_input(harness: AgentTestHarness, bad_lines: list[bytes]):
    proc = harness.mock_process(bad_lines)
    with (
        patch("asyncio.create_subprocess_shell", return_value=proc),
        patch.object(harness.agent, "_initialize", new=AsyncMock()),
    ):
        await harness.agent._run_agent()
    assert harness.agent._done_event.is_set()


# === Timeout Handling ===
async def test_wait_ready_timeout_raises(harness: AgentTestHarness):
    with pytest.raises(TimeoutError):
        await harness.agent.wait_ready(timeout=0.1)


async def test_wait_ready_succeeds_when_set(harness: AgentTestHarness):
    harness.agent._ready_event.set()
    await harness.agent.wait_ready(timeout=0.1)


@pytest.mark.parametrize(
    "options,expected_id,expected_outcome",
    [
        (
            [
                {"kind": "reject_once", "optionId": "reject"},
                {"kind": "allow_once", "optionId": "allow"},
            ],
            "reject",
            None,
        ),
        ([{"kind": "allow_once", "optionId": "allow"}], "allow", None),
        ([], None, "cancelled"),
    ],
)
async def test_permission_timeout_selection(
    harness: AgentTestHarness, options: list, expected_id: str | None, expected_outcome: str | None
):
    harness.agent._auto_approve = False
    harness.agent.post_message = MagicMock(return_value=True)
    with patch("kagan.acp.agent.asyncio.wait_for", side_effect=TimeoutError):
        result = await harness.agent._rpc_request_permission(
            "s1", options, {"toolCallId": "t1", "title": "T"}
        )
    if expected_id:
        assert result["outcome"]["optionId"] == expected_id
    if expected_outcome:
        assert result["outcome"]["outcome"] == expected_outcome


# === Invalid RPC Responses ===
@pytest.mark.parametrize(
    "rpc_request,error_code",
    [
        ({"jsonrpc": "2.0", "method": "unknown/method", "id": 1, "params": {}}, -32601),
        ({"jsonrpc": "2.0", "id": 1, "params": {}}, -32600),
        ({"jsonrpc": "1.0", "method": "session/update", "id": 1, "params": {}}, -32600),
    ],
)
async def test_invalid_rpc_requests(harness: AgentTestHarness, rpc_request: dict, error_code: int):
    result = await harness.agent._server.call(rpc_request)
    assert result is not None and result["error"]["code"] == error_code


async def test_notification_errors_suppressed(harness: AgentTestHarness):
    result = await harness.agent._server.call({"jsonrpc": "2.0", "method": "unknown", "params": {}})
    assert result is None


# === Initialization Errors ===
@pytest.mark.parametrize(
    "error,expected",
    [
        (RPCError("Protocol error", code=-32700), "Failed to initialize"),
        (RuntimeError("Unexpected crash"), "Unexpected crash"),
    ],
)
async def test_initialize_errors(harness: AgentTestHarness, error: Exception, expected: str):
    mock_call = MagicMock(wait=AsyncMock(side_effect=error))
    with patch.object(harness.agent._client, "call", return_value=mock_call):
        await harness.agent._initialize()
    harness.assert_posted_fail(expected)


async def test_initialize_success_posts_ready(harness: AgentTestHarness):
    init_call = MagicMock(wait=AsyncMock(return_value={"agentCapabilities": {}}))
    session_call = MagicMock(wait=AsyncMock(return_value={"sessionId": "test"}))
    calls = iter([init_call, session_call])
    with patch.object(harness.agent._client, "call", side_effect=lambda *a, **kw: next(calls)):
        await harness.agent._initialize()
    harness.assert_posted_ready()
    assert harness.agent._ready_event.is_set()


# === Shutdown & Cleanup ===
@pytest.mark.parametrize("returncode,should_terminate", [(None, True), (0, False)])
async def test_stop_terminates_based_on_state(
    harness: AgentTestHarness, returncode: int | None, should_terminate: bool
):
    proc = MagicMock(returncode=returncode, terminate=MagicMock(), wait=AsyncMock(return_value=0))
    harness.agent._process = proc
    await harness.agent.stop()
    assert proc.terminate.called == should_terminate


async def test_stop_clears_buffers(harness: AgentTestHarness):
    harness.agent._buffers.append_response("test")
    await harness.agent.stop()
    assert harness.agent._buffers.get_response_text() == ""


@pytest.mark.parametrize(
    "wait_error,should_kill", [(None, False), (TimeoutError, True), (ProcessLookupError, False)]
)
async def test_background_cleanup(
    harness: AgentTestHarness, wait_error: type[Exception] | None, should_kill: bool
):
    proc = MagicMock(kill=MagicMock())
    proc.wait = AsyncMock(side_effect=wait_error) if wait_error else AsyncMock(return_value=0)
    harness.agent._process = proc
    await harness.agent._background_cleanup()
    assert proc.kill.called == should_kill


async def test_cancel_sends_notification(harness: AgentTestHarness):
    harness.agent.session_id = "test"
    harness.agent._client.notify = MagicMock()
    assert await harness.agent.cancel() is True
    harness.agent._client.notify.assert_called_once()


# === Message Buffering ===
def test_post_message_without_target_buffers(tmp_path: Path, agent_config: AgentConfig):
    agent = Agent(tmp_path, agent_config)
    agent._message_target = None
    assert agent.post_message(messages.AgentUpdate("text", "test")) is False
    assert len(agent._buffers.messages) == 1


async def test_request_permission_not_buffered(tmp_path: Path, agent_config: AgentConfig):
    agent = Agent(tmp_path, agent_config)
    agent._message_target = None
    future: asyncio.Future[messages.Answer] = asyncio.get_running_loop().create_future()
    agent.post_message(messages.RequestPermission([], {}, future), buffer=True)
    assert len(agent._buffers.messages) == 0


def test_set_message_target_replays(tmp_path: Path, agent_config: AgentConfig):
    agent = Agent(tmp_path, agent_config)
    agent._buffers.buffer_message(messages.AgentUpdate("text", "msg1"))
    agent._buffers.buffer_message(messages.AgentUpdate("text", "msg2"))
    target = MagicMock(post_message=MagicMock(return_value=True))
    agent.set_message_target(target)
    assert target.post_message.call_count == 2


# === RPC Handler Errors ===
def test_read_file_handles_errors(tmp_path: Path, agent_config: AgentConfig):
    agent = Agent(tmp_path, agent_config)
    assert agent._rpc_read_text_file("s1", "nonexistent/path.txt")["content"] == ""
    (tmp_path / "subdir").mkdir()
    assert agent._rpc_read_text_file("s1", "subdir")["content"] == ""


def test_write_file_read_only_raises(tmp_path: Path, agent_config: AgentConfig):
    agent = Agent(tmp_path, agent_config, read_only=True)
    with pytest.raises(ValueError, match="not permitted in read-only mode"):
        agent._rpc_write_text_file("s1", "test.txt", "content")


async def test_terminal_create_read_only_raises(tmp_path: Path, agent_config: AgentConfig):
    agent = Agent(tmp_path, agent_config, read_only=True)
    with pytest.raises(ValueError, match="not permitted in read-only mode"):
        await agent._rpc_terminal_create("echo test")


# === Tool Call State ===
async def test_permission_manages_tool_call_state(harness: AgentTestHarness):
    harness.agent._auto_approve = True
    await harness.agent._rpc_request_permission("s1", [], {"toolCallId": "new", "title": "New"})
    assert harness.agent.tool_calls["new"]["title"] == "New"
    harness.agent.tool_calls["old"] = {"toolCallId": "old", "title": "Old"}
    await harness.agent._rpc_request_permission("s1", [], {"toolCallId": "old", "title": "Updated"})
    assert harness.agent.tool_calls["old"]["title"] == "Updated"
