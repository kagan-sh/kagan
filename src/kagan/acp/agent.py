"""ACP-based agent communication via JSON-RPC over subprocess."""

from __future__ import annotations

import asyncio
import json
import os
from typing import TYPE_CHECKING, Any

from textual import log

from kagan import jsonrpc
from kagan.acp import api, messages, protocol, rpc
from kagan.acp.api import API
from kagan.acp.buffers import AgentBuffers
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

        self.server = jsonrpc.Server()
        self.server.expose_instance(self)

        self._process: asyncio.subprocess.Process | None = None
        self._agent_task: asyncio.Task[None] | None = None
        self._read_task: asyncio.Task[None] | None = None

        self.session_id: str = ""
        self.tool_calls: dict[str, protocol.ToolCall] = {}
        self.agent_capabilities: protocol.AgentCapabilities = {}

        self._message_target: MessagePump | None = None
        self._buffers = AgentBuffers()
        self._terminals = TerminalManager(project_root)

        self._ready_event = asyncio.Event()
        self._done_event = asyncio.Event()
        self._auto_approve = False

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
                    API.process_response(data)
                    continue

            task = asyncio.create_task(self._handle_request(data))
            tasks.add(task)
            task.add_done_callback(tasks.discard)

        log.info(f"[_run_agent] Read loop ended after {line_count} lines")
        self._done_event.set()

    async def _handle_request(self, request: jsonrpc.JSONObject) -> None:
        method = request.get("method", "<no method>")
        log.info(f"[RPC IN] method={method}, id={request.get('id')}")
        log.debug(f"[RPC IN] full request: {request}")

        result = await self.server.call(request)
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
        except jsonrpc.APIError as e:
            log.error(f"[_initialize] ACP handshake failed: {e}")
            self.post_message(messages.AgentFail("Failed to initialize", str(e)))
        except Exception as e:
            import traceback

            tb = traceback.format_exc()
            log.error(f"[_initialize] Unexpected error in handshake: {e}")
            log.error(f"[_initialize] Traceback:\n{tb}")
            self.post_message(messages.AgentFail("Failed to initialize", str(e)))

    def send(self, request: jsonrpc.Request) -> None:
        if self._process and self._process.stdin:
            self._process.stdin.write(b"%s\n" % request.body_json)

    def request(self) -> jsonrpc.Request:
        return API.request(self.send)

    def post_message(self, message: Message, buffer: bool = True) -> bool:
        if self._message_target is not None:
            return self._message_target.post_message(message)

        if buffer and not isinstance(message, messages.RequestPermission):
            self._buffers.buffer_message(message)
        return False

    # RPC endpoints - delegate to rpc module
    @jsonrpc.expose("session/update")
    def rpc_session_update(
        self, sessionId: str, update: protocol.SessionUpdate, _meta: dict[str, Any] | None = None
    ) -> None:
        rpc.handle_session_update(self, sessionId, update, _meta)

    @jsonrpc.expose("session/request_permission")
    async def rpc_request_permission(
        self,
        sessionId: str,
        options: list[protocol.PermissionOption],
        toolCall: protocol.ToolCallUpdatePermissionRequest,
        _meta: dict[str, Any] | None = None,
    ) -> protocol.RequestPermissionResponse:
        return await rpc.handle_request_permission(self, sessionId, options, toolCall, _meta)

    @jsonrpc.expose("fs/read_text_file")
    def rpc_read_text_file(
        self, sessionId: str, path: str, line: int | None = None, limit: int | None = None
    ) -> dict[str, str]:
        return rpc.handle_read_text_file(self, sessionId, path, line, limit)

    @jsonrpc.expose("fs/write_text_file")
    def rpc_write_text_file(self, sessionId: str, path: str, content: str) -> None:
        rpc.handle_write_text_file(self, sessionId, path, content)

    @jsonrpc.expose("terminal/create")
    async def rpc_terminal_create(
        self,
        command: str,
        _meta: dict[str, Any] | None = None,
        args: list[str] | None = None,
        cwd: str | None = None,
        env: list[protocol.EnvVariable] | None = None,
        outputByteLimit: int | None = None,
        sessionId: str | None = None,
    ) -> protocol.CreateTerminalResponse:
        return await rpc.handle_terminal_create(
            self, command, _meta, args, cwd, env, outputByteLimit, sessionId
        )

    @jsonrpc.expose("terminal/output")
    async def rpc_terminal_output(
        self, sessionId: str, terminalId: str, _meta: dict[str, Any] | None = None
    ) -> protocol.TerminalOutputResponse:
        return await rpc.handle_terminal_output(self, sessionId, terminalId, _meta)

    @jsonrpc.expose("terminal/kill")
    def rpc_terminal_kill(
        self, sessionId: str, terminalId: str, _meta: dict[str, Any] | None = None
    ) -> protocol.KillTerminalCommandResponse:
        return rpc.handle_terminal_kill(self, sessionId, terminalId, _meta)

    @jsonrpc.expose("terminal/release")
    def rpc_terminal_release(
        self, sessionId: str, terminalId: str, _meta: dict[str, Any] | None = None
    ) -> protocol.ReleaseTerminalResponse:
        return rpc.handle_terminal_release(self, sessionId, terminalId, _meta)

    @jsonrpc.expose("terminal/wait_for_exit")
    async def rpc_terminal_wait_for_exit(
        self, sessionId: str, terminalId: str, _meta: dict[str, Any] | None = None
    ) -> protocol.WaitForTerminalExitResponse:
        return await rpc.handle_terminal_wait_for_exit(self, sessionId, terminalId, _meta)

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

        with self.request():
            response = api.initialize(
                PROTOCOL_VERSION,
                client_caps,
                {"name": NAME, "title": TITLE, "version": VERSION},
            )

        log.info("[_acp_initialize] Waiting for response...")
        result = await response.wait()
        log.info(f"[_acp_initialize] Received response: {result}")
        if result and (caps := result.get("agentCapabilities")):
            self.agent_capabilities = caps
            log.info(f"[_acp_initialize] Agent capabilities: {caps}")

    async def _acp_new_session(self) -> None:
        cwd = str(self.project_root.absolute())
        log.info(f"[_acp_new_session] Sending session/new request with cwd={cwd}")
        with self.request():
            response = api.session_new(cwd, [])

        log.info("[_acp_new_session] Waiting for response...")
        result = await response.wait()
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
        content: list[protocol.ContentBlock] = [{"type": "text", "text": prompt}]

        with self.request():
            response = api.session_prompt(content, self.session_id)

        result = await response.wait()
        stop_reason = result.get("stopReason") if result else None
        resp_len = len(self.get_response_text())
        log.info(f"Agent response complete. stop_reason={stop_reason}, response_len={resp_len}")
        return stop_reason

    async def set_mode(self, mode_id: str) -> str | None:
        with self.request():
            response = api.session_set_mode(self.session_id, mode_id)

        try:
            await response.wait()
        except jsonrpc.APIError as e:
            return str(e)
        return None

    async def cancel(self) -> bool:
        with self.request():
            api.session_cancel(self.session_id, {})
        return True

    async def stop(self) -> None:
        self._terminals.cleanup_all()
        self._buffers.clear_all()

        if self._process and self._process.returncode is None:
            try:
                self._process.terminate()
                await asyncio.wait_for(self._process.wait(), timeout=SHUTDOWN_TIMEOUT)
            except TimeoutError:
                self._process.kill()
            except ProcessLookupError:
                pass

    def get_response_text(self) -> str:
        return self._buffers.get_response_text()
