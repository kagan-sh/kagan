"""RPC handlers for agent communication."""

from __future__ import annotations

import asyncio
from copy import deepcopy
from typing import TYPE_CHECKING, Any, cast

from textual import log

from kagan.acp import messages, protocol

if TYPE_CHECKING:
    from kagan.acp.agent import Agent


def handle_session_update(
    agent: Agent,
    sessionId: str,
    update: protocol.SessionUpdate,
    _meta: dict[str, Any] | None = None,
) -> None:
    """Handle streaming updates from agent."""
    session_update = update.get("sessionUpdate")

    if session_update == "agent_message_chunk":
        content = update.get("content")
        if content and isinstance(content, dict):
            t = str(content.get("type", ""))
            text = str(content.get("text", ""))
            agent._buffers.append_response(text)
            agent.post_message(messages.AgentUpdate(t, text))

    elif session_update == "agent_thought_chunk":
        content = update.get("content")
        if content and isinstance(content, dict):
            t = str(content.get("type", ""))
            text = str(content.get("text", ""))
            agent.post_message(messages.Thinking(t, text))

    elif session_update == "tool_call":
        tool_call_id = str(update.get("toolCallId", ""))
        agent.tool_calls[tool_call_id] = cast("protocol.ToolCall", update)
        agent.post_message(messages.ToolCall(cast("protocol.ToolCall", update)))

    elif session_update == "tool_call_update":
        tool_call_id = str(update.get("toolCallId", ""))
        if tool_call_id in agent.tool_calls:
            for key, value in update.items():
                if value is not None:
                    cast("dict[str, Any]", agent.tool_calls[tool_call_id])[key] = value
        else:
            new_call: dict[str, Any] = {
                "sessionUpdate": "tool_call",
                "toolCallId": tool_call_id,
                "title": "Tool call",
            }
            for key, value in update.items():
                if value is not None:
                    new_call[key] = value
            agent.tool_calls[tool_call_id] = cast("protocol.ToolCall", new_call)
        agent.post_message(
            messages.ToolCallUpdate(
                deepcopy(agent.tool_calls[tool_call_id]),
                cast("protocol.ToolCallUpdate", update),
            )
        )

    elif session_update == "plan":
        entries = update.get("entries")
        if entries is not None:
            agent.post_message(messages.Plan(cast("list[protocol.PlanEntry]", entries)))

    elif session_update == "available_commands_update":
        cmds = update.get("availableCommands")
        if cmds is not None:
            agent.post_message(
                messages.AvailableCommandsUpdate(cast("list[protocol.AvailableCommand]", cmds))
            )

    elif session_update == "current_mode_update":
        mode_id = update.get("currentModeId")
        if mode_id is not None:
            agent.post_message(messages.ModeUpdate(str(mode_id)))


async def handle_request_permission(
    agent: Agent,
    sessionId: str,
    options: list[protocol.PermissionOption],
    toolCall: protocol.ToolCallUpdatePermissionRequest,
    _meta: dict[str, Any] | None = None,
) -> protocol.RequestPermissionResponse:
    """Agent requests permission - blocks until UI responds or auto-approves."""
    tool_call_id = str(toolCall.get("toolCallId", ""))
    tool_title = toolCall.get("title", "Unknown")
    log.info(f"[RPC] session/request_permission: tool={tool_title}, id={tool_call_id}")
    log.debug(f"[RPC] session/request_permission: options={options}")

    if tool_call_id in agent.tool_calls:
        existing = cast("dict[str, Any]", agent.tool_calls[tool_call_id])
        for key, value in toolCall.items():
            existing[key] = value
    else:
        new_call: dict[str, Any] = {
            "sessionUpdate": "tool_call",
            "toolCallId": tool_call_id,
            "title": toolCall.get("title", "Tool call"),
        }
        for key, value in toolCall.items():
            if key != "sessionUpdate":
                new_call[key] = value
        agent.tool_calls[tool_call_id] = cast("protocol.ToolCall", new_call)

    if agent._auto_approve or agent._message_target is None:
        reason = "auto_approve enabled" if agent._auto_approve else "no UI"
        log.info(f"[RPC] session/request_permission: {reason}, auto-approving")
        for opt in options:
            if "allow" in opt.get("kind", ""):
                log.debug(f"[RPC] session/request_permission: auto-selected {opt['optionId']}")
                return {"outcome": {"optionId": opt["optionId"], "outcome": "selected"}}
        if options:
            opt_id = options[0]["optionId"]
            log.debug(f"[RPC] session/request_permission: no allow option, using: {opt_id}")
            return {"outcome": {"optionId": options[0]["optionId"], "outcome": "selected"}}
        log.warning("[RPC] session/request_permission: no options provided!")
        return {"outcome": {"optionId": "", "outcome": "selected"}}

    log.info("[RPC] session/request_permission: waiting for UI response")
    result_future: asyncio.Future[messages.Answer] = asyncio.Future()
    agent.post_message(
        messages.RequestPermission(options, deepcopy(agent.tool_calls[tool_call_id]), result_future)
    )

    try:
        answer = await asyncio.wait_for(result_future, timeout=330.0)
    except TimeoutError:
        log.warning("[RPC] session/request_permission: timeout, auto-rejecting")
        for opt in options:
            if "reject" in opt.get("kind", ""):
                return {"outcome": {"optionId": opt["optionId"], "outcome": "selected"}}
        if options:
            return {"outcome": {"optionId": options[0]["optionId"], "outcome": "selected"}}
        return {"outcome": {"outcome": "cancelled"}}

    log.info(f"[RPC] session/request_permission: UI responded with {answer.id}")
    return {"outcome": {"optionId": answer.id, "outcome": "selected"}}


def handle_read_text_file(
    agent: Agent,
    sessionId: str,
    path: str,
    line: int | None = None,
    limit: int | None = None,
) -> dict[str, str]:
    """Read a file in the project."""
    log.info(f"[RPC] fs/read_text_file: path={path}, line={line}, limit={limit}")
    read_path = agent.project_root / path
    try:
        text = read_path.read_text(encoding="utf-8", errors="ignore")
        log.debug(f"[RPC] fs/read_text_file: read {len(text)} chars from {read_path}")
    except OSError as e:
        log.warning(f"[RPC] fs/read_text_file: failed to read {read_path}: {e}")
        text = ""

    if line is not None:
        line = max(0, line - 1)
        lines = text.splitlines()
        text = "\n".join(lines[line:]) if limit is None else "\n".join(lines[line : line + limit])

    return {"content": text}


def handle_write_text_file(agent: Agent, sessionId: str, path: str, content: str) -> None:
    """Write a file in the project."""
    if agent._read_only:
        log.warning(f"[RPC] fs/write_text_file: BLOCKED in read-only mode (path={path})")
        raise ValueError("Write operations not permitted in read-only mode")

    log.info(f"[RPC] fs/write_text_file: path={path}, content_len={len(content)}")
    write_path = agent.project_root / path
    log.debug(f"[RPC] fs/write_text_file: writing to {write_path}")
    write_path.parent.mkdir(parents=True, exist_ok=True)
    write_path.write_text(content, encoding="utf-8")
    log.info(f"[RPC] fs/write_text_file: successfully wrote {len(content)} chars to {write_path}")


async def handle_terminal_create(
    agent: Agent,
    command: str,
    _meta: dict[str, Any] | None = None,
    args: list[str] | None = None,
    cwd: str | None = None,
    env: list[protocol.EnvVariable] | None = None,
    outputByteLimit: int | None = None,
    sessionId: str | None = None,
) -> protocol.CreateTerminalResponse:
    """Agent wants to create a terminal."""
    if agent._read_only:
        log.warning(f"[RPC] terminal/create: BLOCKED in read-only mode (command={command})")
        raise ValueError("Terminal operations not permitted in read-only mode")

    terminal_id, cmd_display = await agent._terminals.create(
        command=command,
        args=args,
        cwd=cwd,
        env=env,
        output_byte_limit=outputByteLimit,
    )
    agent.post_message(messages.AgentUpdate("terminal", f"$ {cmd_display}"))
    return {"terminalId": terminal_id}


async def handle_terminal_output(
    agent: Agent,
    sessionId: str,
    terminalId: str,
    _meta: dict[str, Any] | None = None,
) -> protocol.TerminalOutputResponse:
    """Get terminal output."""
    return agent._terminals.get_output(terminalId)


def handle_terminal_kill(
    agent: Agent,
    sessionId: str,
    terminalId: str,
    _meta: dict[str, Any] | None = None,
) -> protocol.KillTerminalCommandResponse:
    """Kill a terminal."""
    agent._terminals.kill(terminalId)
    return {}


def handle_terminal_release(
    agent: Agent,
    sessionId: str,
    terminalId: str,
    _meta: dict[str, Any] | None = None,
) -> protocol.ReleaseTerminalResponse:
    """Release a terminal."""
    agent._terminals.release(terminalId)
    return {}


async def handle_terminal_wait_for_exit(
    agent: Agent,
    sessionId: str,
    terminalId: str,
    _meta: dict[str, Any] | None = None,
) -> protocol.WaitForTerminalExitResponse:
    """Wait for terminal to exit."""
    return_code, signal = await agent._terminals.wait_for_exit(terminalId)

    final_output = agent._terminals.get_final_output(terminalId)
    if final_output.strip():
        agent.post_message(messages.AgentUpdate("terminal_output", final_output))
    status = "success" if return_code == 0 else "error"
    agent.post_message(messages.AgentUpdate("terminal_exit", f"[{status}] Exit: {return_code}"))

    return {"exitCode": return_code, "signal": signal}
