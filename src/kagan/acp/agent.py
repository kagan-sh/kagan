"""ACP-based agent communication via JSON-RPC over subprocess."""

from __future__ import annotations

import asyncio
import contextlib
import json
import os
from copy import deepcopy
from typing import TYPE_CHECKING, Any

from textual import log

from kagan.acp import messages, protocol
from kagan.acp.buffers import AgentBuffers
from kagan.acp.jsonrpc import Client, RPCError, Server
from kagan.acp.terminals import TerminalManager
from kagan.limits import SHUTDOWN_TIMEOUT, SUBPROCESS_LIMIT

if TYPE_CHECKING:
    from pathlib import Path

    from textual.message import Message
    from textual.message_pump import MessagePump

    from kagan.config import AgentConfig

PROTOCOL_VERSION = 1
NAME = "kagan"
TITLE = "Kagan"
VERSION = "0.1.0"


class Agent:
    """ACP-based agent communication via JSON-RPC over subprocess."""

    def __init__(
        self, project_root: Path, agent_config: AgentConfig, *, read_only: bool = False
    ) -> None:
        self.project_root = project_root
        self._agent_config = agent_config
        self._read_only = read_only

        # JSON-RPC server for incoming requests
        self._server = Server()
        self._register_rpc_methods()

        # JSON-RPC client for outgoing requests
        self._client = Client()

        self._process: asyncio.subprocess.Process | None = None
        self._agent_task: asyncio.Task[None] | None = None
        self._read_task: asyncio.Task[None] | None = None
        self._cleanup_task: asyncio.Task[None] | None = None

        self.session_id: str = ""
        self.tool_calls: dict[str, protocol.ToolCall] = {}
        self.agent_capabilities: protocol.AgentCapabilities = {}

        self._message_target: MessagePump | None = None
        self._buffers = AgentBuffers()
        self._terminals = TerminalManager(project_root)

        self._ready_event = asyncio.Event()
        self._done_event = asyncio.Event()
        self._auto_approve = False

    def _register_rpc_methods(self) -> None:
        """Register RPC method handlers."""
        self._server.register("session/update", self._rpc_session_update)
        self._server.register("session/request_permission", self._rpc_request_permission)
        self._server.register("fs/read_text_file", self._rpc_read_text_file)
        self._server.register("fs/write_text_file", self._rpc_write_text_file)
        self._server.register("terminal/create", self._rpc_terminal_create)
        self._server.register("terminal/output", self._rpc_terminal_output)
        self._server.register("terminal/kill", self._rpc_terminal_kill)
        self._server.register("terminal/release", self._rpc_terminal_release)
        self._server.register("terminal/wait_for_exit", self._rpc_terminal_wait_for_exit)

    @property
    def server(self) -> Server:
        """Expose server for testing."""
        return self._server

    @property
    def command(self) -> str | None:
        from kagan import get_os_value
        from kagan.ui.screens.troubleshooting import resolve_acp_command

        raw_command = get_os_value(self._agent_config.run_command)
        if raw_command is None:
            return None

        # Use smart resolution to handle npx fallback
        resolution = resolve_acp_command(raw_command, self._agent_config.name)
        return resolution.resolved_command

    def set_message_target(self, target: MessagePump | None) -> None:
        self._message_target = target
        if target is not None and self._buffers.messages:
            log.debug(f"Replaying {len(self._buffers.messages)} buffered messages to new target")
            self._buffers.replay_messages_to(target)

    def set_auto_approve(self, enabled: bool) -> None:
        self._auto_approve = enabled
        log.debug(f"Auto-approve mode: {enabled}")

    def start(self, message_target: MessagePump | None = None) -> None:
        log.info(f"Starting agent for project: {self.project_root}")
        log.debug(f"Agent config: {self._agent_config}")
        self._message_target = message_target
        self._agent_task = asyncio.create_task(self._run_agent())

    def _send_bytes(self, data: bytes) -> None:
        """Send raw bytes to the agent process."""
        if self._process and self._process.stdin:
            self._process.stdin.write(data)

    async def _run_agent(self) -> None:
        log.info(f"[_run_agent] Starting for project: {self.project_root}")
        PIPE = asyncio.subprocess.PIPE
        env = os.environ.copy()
        env["KAGAN_CWD"] = str(self.project_root.absolute())

        command = self.command
        if command is None:
            log.error("[_run_agent] No run command for this OS")
            self.post_message(messages.AgentFail("No run command for this OS"))
            return

        log.info(f"[_run_agent] Spawning agent process: {command}")
        log.info(f"[_run_agent] Working directory: {self.project_root}")
        log.info(f"[_run_agent] KAGAN_CWD={env['KAGAN_CWD']}")

        try:
            log.info("[_run_agent] Calling create_subprocess_shell...")
            abs_cwd = str(self.project_root.absolute())
            self._process = await asyncio.create_subprocess_shell(
                command,
                stdin=PIPE,
                stdout=PIPE,
                stderr=PIPE,
                env=env,
                cwd=abs_cwd,
                limit=SUBPROCESS_LIMIT,
            )
            log.info(f"[_run_agent] Agent process started with PID: {self._process.pid}")
        except Exception as e:
            import traceback

            tb = traceback.format_exc()
            log.error(f"[_run_agent] Failed to start agent: {e}")
            log.error(f"[_run_agent] Traceback:\n{tb}")
            self.post_message(messages.AgentFail("Failed to start agent", str(e)))
            return

        # Wire up the client sender now that we have a process
        self._client.set_sender(self._send_bytes)

        log.info("[_run_agent] Starting initialization task...")
        self._read_task = asyncio.create_task(self._initialize())

        assert self._process.stdout is not None
        tasks: set[asyncio.Task[None]] = set()

        log.info("[_run_agent] Entering main read loop...")
        line_count = 0
        while line := await self._process.stdout.readline():
            line_count += 1
            if not line.strip():
                continue

            try:
                data = json.loads(line.decode("utf-8"))
                log.debug(f"[_run_agent] Received line #{line_count}: {str(data)[:200]}")
            except (json.JSONDecodeError, UnicodeDecodeError) as e:
                log.warning(f"[_run_agent] Failed to parse line #{line_count}: {e}")
                continue

            if isinstance(data, dict):
                if "result" in data or "error" in data:
                    self._client.handle_response(data)
                    continue

            task = asyncio.create_task(self._handle_request(data))
            tasks.add(task)
            task.add_done_callback(tasks.discard)

        log.info(f"[_run_agent] Read loop ended after {line_count} lines")
        self._done_event.set()

    async def _handle_request(self, request: dict[str, Any]) -> None:
        method = request.get("method", "<no method>")
        log.info(f"[RPC IN] method={method}, id={request.get('id')}")
        log.debug(f"[RPC IN] full request: {request}")

        result = await self._server.call(request)
        if result is not None and self._process and self._process.stdin:
            result_json = json.dumps(result).encode("utf-8")
            self._process.stdin.write(b"%s\n" % result_json)

    async def _initialize(self) -> None:
        log.info("[_initialize] Starting ACP handshake...")
        try:
            log.info("[_initialize] Sending acp_initialize request...")
            await self._acp_initialize()
            log.info("[_initialize] acp_initialize complete, sending acp_new_session...")
            await self._acp_new_session()
            log.info(f"[_initialize] ACP handshake complete, session_id={self.session_id}")
            self._ready_event.set()
            self.post_message(messages.AgentReady())
        except RPCError as e:
            log.error(f"[_initialize] ACP handshake failed: {e}")
            self.post_message(messages.AgentFail("Failed to initialize", str(e)))
        except Exception as e:
            import traceback

            tb = traceback.format_exc()
            log.error(f"[_initialize] Unexpected error in handshake: {e}")
            log.error(f"[_initialize] Traceback:\n{tb}")
            self.post_message(messages.AgentFail("Failed to initialize", str(e)))

    def post_message(self, message: Message, buffer: bool = True) -> bool:
        if self._message_target is not None:
            return self._message_target.post_message(message)

        if buffer and not isinstance(message, messages.RequestPermission):
            self._buffers.buffer_message(message)
        return False

    # RPC method handlers (inlined from rpc.py)

    def _rpc_session_update(
        self, sessionId: str, update: protocol.SessionUpdate, _meta: dict[str, Any] | None = None
    ) -> None:
        """Handle streaming updates from agent."""
        session_update = update.get("sessionUpdate")

        if session_update == "agent_message_chunk":
            content = update.get("content")
            if content and isinstance(content, dict):
                t = str(content.get("type", ""))
                text = str(content.get("text", ""))
                self._buffers.append_response(text)
                self.post_message(messages.AgentUpdate(t, text))

        elif session_update == "agent_thought_chunk":
            content = update.get("content")
            if content and isinstance(content, dict):
                t = str(content.get("type", ""))
                text = str(content.get("text", ""))
                self.post_message(messages.Thinking(t, text))

        elif session_update == "tool_call":
            tool_call_id = str(update.get("toolCallId", ""))
            self.tool_calls[tool_call_id] = update
            self.post_message(messages.ToolCall(update))

        elif session_update == "tool_call_update":
            tool_call_id = str(update.get("toolCallId", ""))
            if tool_call_id in self.tool_calls:
                for key, value in update.items():
                    if value is not None:
                        self.tool_calls[tool_call_id][key] = value
            else:
                new_call: dict[str, Any] = {
                    "sessionUpdate": "tool_call",
                    "toolCallId": tool_call_id,
                    "title": "Tool call",
                }
                for key, value in update.items():
                    if value is not None:
                        new_call[key] = value
                self.tool_calls[tool_call_id] = new_call
            self.post_message(
                messages.ToolCallUpdate(
                    deepcopy(self.tool_calls[tool_call_id]),
                    update,
                )
            )

        elif session_update == "plan":
            entries = update.get("entries")
            if entries is not None:
                self.post_message(messages.Plan(entries))

        elif session_update == "available_commands_update":
            cmds = update.get("availableCommands")
            if cmds is not None:
                self.post_message(messages.AvailableCommandsUpdate(cmds))

        elif session_update == "current_mode_update":
            mode_id = update.get("currentModeId")
            if mode_id is not None:
                self.post_message(messages.ModeUpdate(str(mode_id)))

    async def _rpc_request_permission(
        self,
        sessionId: str,
        options: list[protocol.PermissionOption],
        toolCall: protocol.ToolCallUpdatePermissionRequest,
        _meta: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Agent requests permission - blocks until UI responds or auto-approves."""
        tool_call_id = str(toolCall.get("toolCallId", ""))
        tool_title = toolCall.get("title", "Unknown")
        log.info(f"[RPC] session/request_permission: tool={tool_title}, id={tool_call_id}")
        log.debug(f"[RPC] session/request_permission: options={options}")

        if tool_call_id in self.tool_calls:
            existing = self.tool_calls[tool_call_id]
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
            self.tool_calls[tool_call_id] = new_call

        if self._auto_approve or self._message_target is None:
            reason = "auto_approve enabled" if self._auto_approve else "no UI"
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
        self.post_message(
            messages.RequestPermission(
                options, deepcopy(self.tool_calls[tool_call_id]), result_future
            )
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

    def _rpc_read_text_file(
        self, sessionId: str, path: str, line: int | None = None, limit: int | None = None
    ) -> dict[str, str]:
        """Read a file in the project."""
        log.info(f"[RPC] fs/read_text_file: path={path}, line={line}, limit={limit}")
        read_path = self.project_root / path
        try:
            text = read_path.read_text(encoding="utf-8", errors="ignore")
            log.debug(f"[RPC] fs/read_text_file: read {len(text)} chars from {read_path}")
        except OSError as e:
            log.warning(f"[RPC] fs/read_text_file: failed to read {read_path}: {e}")
            text = ""

        if line is not None:
            line = max(0, line - 1)
            lines = text.splitlines()
            text = (
                "\n".join(lines[line:]) if limit is None else "\n".join(lines[line : line + limit])
            )

        return {"content": text}

    def _rpc_write_text_file(self, sessionId: str, path: str, content: str) -> None:
        """Write a file in the project."""
        if self._read_only:
            log.warning(f"[RPC] fs/write_text_file: BLOCKED in read-only mode (path={path})")
            raise ValueError("Write operations not permitted in read-only mode")

        log.info(f"[RPC] fs/write_text_file: path={path}, content_len={len(content)}")
        write_path = self.project_root / path
        log.debug(f"[RPC] fs/write_text_file: writing to {write_path}")
        write_path.parent.mkdir(parents=True, exist_ok=True)
        write_path.write_text(content, encoding="utf-8")
        log.info(
            f"[RPC] fs/write_text_file: successfully wrote {len(content)} chars to {write_path}"
        )

    async def _rpc_terminal_create(
        self,
        command: str,
        _meta: dict[str, Any] | None = None,
        args: list[str] | None = None,
        cwd: str | None = None,
        env: list[protocol.EnvVariable] | None = None,
        outputByteLimit: int | None = None,
        sessionId: str | None = None,
    ) -> dict[str, str]:
        """Agent wants to create a terminal."""
        if self._read_only:
            log.warning(f"[RPC] terminal/create: BLOCKED in read-only mode (command={command})")
            raise ValueError("Terminal operations not permitted in read-only mode")

        terminal_id, cmd_display = await self._terminals.create(
            command=command,
            args=args,
            cwd=cwd,
            env=env,
            output_byte_limit=outputByteLimit,
        )
        self.post_message(messages.AgentUpdate("terminal", f"$ {cmd_display}"))
        return {"terminalId": terminal_id}

    async def _rpc_terminal_output(
        self, sessionId: str, terminalId: str, _meta: dict[str, Any] | None = None
    ) -> protocol.TerminalOutputResponse:
        """Get terminal output."""
        return self._terminals.get_output(terminalId)

    def _rpc_terminal_kill(
        self, sessionId: str, terminalId: str, _meta: dict[str, Any] | None = None
    ) -> dict[str, Any]:
        """Kill a terminal."""
        self._terminals.kill(terminalId)
        return {}

    def _rpc_terminal_release(
        self, sessionId: str, terminalId: str, _meta: dict[str, Any] | None = None
    ) -> dict[str, Any]:
        """Release a terminal."""
        self._terminals.release(terminalId)
        return {}

    async def _rpc_terminal_wait_for_exit(
        self, sessionId: str, terminalId: str, _meta: dict[str, Any] | None = None
    ) -> dict[str, Any]:
        """Wait for terminal to exit."""
        return_code, signal = await self._terminals.wait_for_exit(terminalId)

        final_output = self._terminals.get_final_output(terminalId)
        if final_output.strip():
            self.post_message(messages.AgentUpdate("terminal_output", final_output))
        status = "success" if return_code == 0 else "error"
        self.post_message(messages.AgentUpdate("terminal_exit", f"[{status}] Exit: {return_code}"))

        return {"exitCode": return_code, "signal": signal}

    # ACP protocol methods
    async def _acp_initialize(self) -> None:
        log.info("[_acp_initialize] Sending initialize request to agent...")

        # Build capabilities based on read_only mode
        fs_caps: protocol.FileSystemCapability = {"readTextFile": True}
        if not self._read_only:
            fs_caps["writeTextFile"] = True

        client_caps: protocol.ClientCapabilities = {
            "fs": fs_caps,
            "terminal": not self._read_only,
        }
        log.info(f"[_acp_initialize] Capabilities: read_only={self._read_only}, caps={client_caps}")

        call = self._client.call(
            "initialize",
            protocolVersion=PROTOCOL_VERSION,
            clientCapabilities=client_caps,
            clientInfo={"name": NAME, "title": TITLE, "version": VERSION},
        )

        log.info("[_acp_initialize] Waiting for response...")
        result = await call.wait()
        log.info(f"[_acp_initialize] Received response: {result}")
        if result and (caps := result.get("agentCapabilities")):
            self.agent_capabilities = caps
            log.info(f"[_acp_initialize] Agent capabilities: {caps}")

    async def _acp_new_session(self) -> None:
        cwd = str(self.project_root.absolute())
        log.info(f"[_acp_new_session] Sending session/new request with cwd={cwd}")

        call = self._client.call("session/new", cwd=cwd, mcpServers=[])

        log.info("[_acp_new_session] Waiting for response...")
        result = await call.wait()
        assert result is not None
        self.session_id = result["sessionId"]
        log.info(f"[_acp_new_session] Session created: {self.session_id}")

        if modes := result.get("modes"):
            current_mode = modes["currentModeId"]
            available_modes = modes["availableModes"]
            modes_dict = {
                m["id"]: messages.Mode(m["id"], m["name"], m.get("description"))
                for m in available_modes
            }
            self.post_message(messages.SetModes(current_mode, modes_dict))

    # Public API
    async def wait_ready(self, timeout: float = 30.0) -> None:
        log.info(f"[wait_ready] Waiting for agent ready event (timeout={timeout}s)...")
        try:
            async with asyncio.timeout(timeout):
                await self._ready_event.wait()
            log.info("[wait_ready] Agent is ready!")
        except TimeoutError:
            log.error(f"[wait_ready] Timeout after {timeout}s waiting for agent")
            raise

    async def send_prompt(self, prompt: str) -> str | None:
        log.info(f"Sending prompt to agent (len={len(prompt)})")
        log.debug(f"Prompt content: {prompt[:500]}...")
        self._buffers.clear_response()
        self.tool_calls.clear()
        content: list[dict[str, Any]] = [{"type": "text", "text": prompt}]

        call = self._client.call("session/prompt", prompt=content, sessionId=self.session_id)

        result = await call.wait()
        stop_reason = result.get("stopReason") if result else None
        resp_len = len(self.get_response_text())
        log.info(f"Agent response complete. stop_reason={stop_reason}, response_len={resp_len}")
        self.post_message(messages.AgentComplete())
        return stop_reason

    async def set_mode(self, mode_id: str) -> str | None:
        call = self._client.call("session/set_mode", sessionId=self.session_id, modeId=mode_id)

        try:
            await call.wait()
        except RPCError as e:
            return str(e)
        return None

    async def cancel(self) -> bool:
        self._client.notify("session/cancel", sessionId=self.session_id, _meta={})
        return True

    async def _background_cleanup(self) -> None:
        """Background task to wait for process termination after stop."""
        if self._process is None:
            return
        try:
            await asyncio.wait_for(self._process.wait(), timeout=SHUTDOWN_TIMEOUT)
        except TimeoutError:
            with contextlib.suppress(ProcessLookupError):
                self._process.kill()
        except ProcessLookupError:
            pass

    async def stop(self) -> None:
        """Stop the agent process gracefully (non-blocking).

        Terminates the process and schedules cleanup in the background,
        returning immediately to avoid blocking the UI.
        """
        self._terminals.cleanup_all()
        self._buffers.clear_all()

        if self._process and self._process.returncode is None:
            self._process.terminate()
            # Fire-and-forget: schedule cleanup without blocking caller
            # Store reference to satisfy RUF006 (dangling task)
            self._cleanup_task = asyncio.create_task(self._background_cleanup())

    def get_response_text(self) -> str:
        return self._buffers.get_response_text()
