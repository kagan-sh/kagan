"""ACP-based agent communication via JSON-RPC over subprocess."""

from __future__ import annotations

import asyncio
import json
import os
from copy import deepcopy
from pathlib import Path  # noqa: TC003 - used at runtime
from typing import TYPE_CHECKING, Any, cast

from textual import log

from kagan import jsonrpc
from kagan.acp import api, messages, protocol
from kagan.acp.api import API
from kagan.acp.terminal import TerminalRunner

if TYPE_CHECKING:
    from textual.message import Message
    from textual.message_pump import MessagePump

    from kagan.config import AgentConfig

# Protocol constants
PROTOCOL_VERSION = 1
NAME = "kagan"
TITLE = "Kagan"
VERSION = "0.1.0"


class Agent:
    """ACP-based agent communication via JSON-RPC over subprocess."""

    def __init__(self, project_root: Path, agent_config: AgentConfig) -> None:
        """Initialize the agent.

        Args:
            project_root: Path to the project root directory.
            agent_config: Configuration for the agent.
        """
        self.project_root = project_root
        self._agent_config = agent_config

        # JSON-RPC server for incoming requests
        self.server = jsonrpc.Server()
        self.server.expose_instance(self)

        # Process state
        self._process: asyncio.subprocess.Process | None = None
        self._agent_task: asyncio.Task[None] | None = None
        self._read_task: asyncio.Task[None] | None = None

        # Session state
        self.session_id: str = ""
        self.tool_calls: dict[str, protocol.ToolCall] = {}
        self.agent_capabilities: protocol.AgentCapabilities = {}

        # Message dispatch target (usually the conversation/screen)
        self._message_target: MessagePump | None = None

        # Terminal tracking - managed directly in Agent
        self._terminal_count: int = 0
        self._terminals: dict[str, TerminalRunner] = {}

        # Response accumulation
        self._response_text: list[str] = []

        # Message buffer for when no target is connected
        # Stores (message, is_blocking) tuples - blocking messages need special handling
        self._message_buffer: list[Message] = []
        self._buffer_limit = 500  # Keep last N messages

        # Events
        self._ready_event = asyncio.Event()
        self._done_event = asyncio.Event()

    @property
    def command(self) -> str | None:
        """Get OS-specific run command."""
        from kagan import get_os_value

        return get_os_value(self._agent_config.run_command)

    # --- Lifecycle methods ---

    def set_message_target(self, target: MessagePump | None) -> None:
        """Set or update the message target for UI updates.

        This can be called after start() to begin receiving messages,
        or to change the target (e.g., when opening a new modal).

        When a target is set, any buffered messages are replayed to it.

        Args:
            target: The Textual widget to receive messages, or None to stop.
        """
        self._message_target = target

        # Replay buffered messages to new target
        if target is not None and self._message_buffer:
            log.debug(f"Replaying {len(self._message_buffer)} buffered messages to new target")
            for msg in self._message_buffer:
                target.post_message(msg)
            self._message_buffer.clear()

    def start(self, message_target: MessagePump | None = None) -> None:
        """Start the agent subprocess.

        Args:
            message_target: Target for posting UI messages.
        """
        log.info(f"Starting agent for project: {self.project_root}")
        log.debug(f"Agent config: {self._agent_config}")
        self._message_target = message_target
        self._agent_task = asyncio.create_task(self._run_agent())

    async def _run_agent(self) -> None:
        """Main task to communicate with agent subprocess."""
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
                limit=10 * 1024 * 1024,  # 10MB buffer
            )
            log.info(f"[_run_agent] Agent process started with PID: {self._process.pid}")
        except Exception as e:
            import traceback

            tb = traceback.format_exc()
            log.error(f"[_run_agent] Failed to start agent: {e}")
            log.error(f"[_run_agent] Traceback:\n{tb}")
            self.post_message(messages.AgentFail("Failed to start agent", str(e)))
            return

        # Start initialization
        log.info("[_run_agent] Starting initialization task...")
        self._read_task = asyncio.create_task(self._initialize())

        # Main read loop
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

            # Route: response vs incoming request
            if isinstance(data, dict):
                if "result" in data or "error" in data:
                    API.process_response(data)
                    continue

            # Incoming RPC request from agent
            task = asyncio.create_task(self._handle_request(data))
            tasks.add(task)
            task.add_done_callback(tasks.discard)

        log.info(f"[_run_agent] Read loop ended after {line_count} lines")
        self._done_event.set()

    async def _handle_request(self, request: jsonrpc.JSONObject) -> None:
        """Handle incoming JSON-RPC request from agent.

        Args:
            request: The JSON-RPC request object.
        """
        # Log incoming RPC method calls for debugging
        method = request.get("method", "<no method>")
        log.info(f"[RPC IN] method={method}, id={request.get('id')}")
        log.debug(f"[RPC IN] full request: {request}")

        result = await self.server.call(request)
        if result is not None and self._process and self._process.stdin:
            result_json = json.dumps(result).encode("utf-8")
            self._process.stdin.write(b"%s\n" % result_json)

    async def _initialize(self) -> None:
        """ACP handshake: initialize + new_session."""
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

    # --- Request sending ---

    def send(self, request: jsonrpc.Request) -> None:
        """Send a request to the agent subprocess.

        Args:
            request: The JSON-RPC request to send.
        """
        if self._process and self._process.stdin:
            self._process.stdin.write(b"%s\n" % request.body_json)

    def request(self) -> jsonrpc.Request:
        """Create a request context that auto-sends on exit.

        Returns:
            Request context manager.
        """
        return API.request(self.send)

    def post_message(self, message: Message, buffer: bool = True) -> bool:
        """Post message to UI target, or buffer if no target connected.

        Args:
            message: The message to post.
            buffer: If True, buffer the message when no target is connected.
                    Set to False for messages that shouldn't be replayed.

        Returns:
            True if message was posted to target, False if buffered or dropped.
        """
        if self._message_target is not None:
            return self._message_target.post_message(message)

        # Buffer non-blocking messages for replay when target connects
        if buffer and not isinstance(message, messages.RequestPermission):
            self._message_buffer.append(message)
            # Trim buffer if too large
            if len(self._message_buffer) > self._buffer_limit:
                self._message_buffer = self._message_buffer[-self._buffer_limit :]
        return False

    # --- Exposed RPC endpoints (Agent calls these) ---

    @jsonrpc.expose("session/update")
    def rpc_session_update(
        self,
        sessionId: str,
        update: protocol.SessionUpdate,
        _meta: dict[str, Any] | None = None,
    ) -> None:
        """Handle streaming updates from agent.

        Args:
            sessionId: The session ID.
            update: The session update data.
            _meta: Optional metadata.
        """
        session_update = update.get("sessionUpdate")

        if session_update == "agent_message_chunk":
            content = update.get("content")
            if content and isinstance(content, dict):
                t = str(content.get("type", ""))
                text = str(content.get("text", ""))
                self._response_text.append(text)
                self.post_message(messages.AgentUpdate(t, text))

        elif session_update == "agent_thought_chunk":
            content = update.get("content")
            if content and isinstance(content, dict):
                t = str(content.get("type", ""))
                text = str(content.get("text", ""))
                self.post_message(messages.Thinking(t, text))

        elif session_update == "tool_call":
            tool_call_id = str(update.get("toolCallId", ""))
            self.tool_calls[tool_call_id] = cast("protocol.ToolCall", update)
            self.post_message(messages.ToolCall(cast("protocol.ToolCall", update)))

        elif session_update == "tool_call_update":
            tool_call_id = str(update.get("toolCallId", ""))
            if tool_call_id in self.tool_calls:
                for key, value in update.items():
                    if value is not None:
                        cast("dict[str, Any]", self.tool_calls[tool_call_id])[key] = value
            else:
                # Agent sent update without prior tool_call
                new_call: dict[str, Any] = {
                    "sessionUpdate": "tool_call",
                    "toolCallId": tool_call_id,
                    "title": "Tool call",
                }
                for key, value in update.items():
                    if value is not None:
                        new_call[key] = value
                self.tool_calls[tool_call_id] = cast("protocol.ToolCall", new_call)
            self.post_message(
                messages.ToolCallUpdate(
                    deepcopy(self.tool_calls[tool_call_id]),
                    cast("protocol.ToolCallUpdate", update),
                )
            )

        elif session_update == "plan":
            entries = update.get("entries")
            if entries is not None:
                self.post_message(messages.Plan(cast("list[protocol.PlanEntry]", entries)))

        elif session_update == "available_commands_update":
            cmds = update.get("availableCommands")
            if cmds is not None:
                self.post_message(
                    messages.AvailableCommandsUpdate(cast("list[protocol.AvailableCommand]", cmds))
                )

        elif session_update == "current_mode_update":
            mode_id = update.get("currentModeId")
            if mode_id is not None:
                self.post_message(messages.ModeUpdate(str(mode_id)))

    @jsonrpc.expose("session/request_permission")
    async def rpc_request_permission(
        self,
        sessionId: str,
        options: list[protocol.PermissionOption],
        toolCall: protocol.ToolCallUpdatePermissionRequest,
        _meta: dict[str, Any] | None = None,
    ) -> protocol.RequestPermissionResponse:
        """Agent requests permission - blocks until UI responds or auto-approves.

        Args:
            sessionId: The session ID.
            options: Available permission options.
            toolCall: The tool call requesting permission.
            _meta: Optional metadata.

        Returns:
            The permission response.
        """
        tool_call_id = str(toolCall.get("toolCallId", ""))
        tool_title = toolCall.get("title", "Unknown")
        log.info(f"[RPC] session/request_permission: tool={tool_title}, id={tool_call_id}")
        log.debug(f"[RPC] session/request_permission: options={options}")

        if tool_call_id in self.tool_calls:
            # Merge toolCall fields into existing tool_calls entry
            existing = cast("dict[str, Any]", self.tool_calls[tool_call_id])
            for key, value in toolCall.items():
                existing[key] = value
        else:
            # Create new ToolCall from permission request
            new_call: dict[str, Any] = {
                "sessionUpdate": "tool_call",
                "toolCallId": tool_call_id,
                "title": toolCall.get("title", "Tool call"),
            }
            for key, value in toolCall.items():
                if key != "sessionUpdate":  # Keep our "tool_call" sessionUpdate
                    new_call[key] = value
            self.tool_calls[tool_call_id] = cast("protocol.ToolCall", new_call)

        # If no UI is connected, auto-approve with first "allow" option
        if self._message_target is None:
            log.info("[RPC] session/request_permission: no UI, auto-approving")
            for opt in options:
                if "allow" in opt.get("kind", ""):
                    log.debug(
                        f"[RPC] session/request_permission: auto-selected option {opt['optionId']}"
                    )
                    return {"outcome": {"optionId": opt["optionId"], "outcome": "selected"}}
            # No allow option, use first option
            if options:
                opt_id = options[0]["optionId"]
                log.debug(f"[RPC] session/request_permission: no allow option, using: {opt_id}")
                return {"outcome": {"optionId": options[0]["optionId"], "outcome": "selected"}}
            # No options at all - this shouldn't happen
            log.warning("[RPC] session/request_permission: no options provided!")
            return {"outcome": {"optionId": "", "outcome": "selected"}}

        # Post to UI and wait for response
        log.info("[RPC] session/request_permission: waiting for UI response")
        result_future: asyncio.Future[messages.Answer] = asyncio.Future()
        self.post_message(
            messages.RequestPermission(
                options, deepcopy(self.tool_calls[tool_call_id]), result_future
            )
        )

        answer = await result_future
        log.info(f"[RPC] session/request_permission: UI responded with {answer.id}")
        return {"outcome": {"optionId": answer.id, "outcome": "selected"}}

    @jsonrpc.expose("fs/read_text_file")
    def rpc_read_text_file(
        self,
        sessionId: str,
        path: str,
        line: int | None = None,
        limit: int | None = None,
    ) -> dict[str, str]:
        """Read a file in the project.

        Args:
            sessionId: The session ID.
            path: Path to the file relative to project root.
            line: Optional starting line number (1-indexed).
            limit: Optional number of lines to read.

        Returns:
            Dict with 'content' key containing file text.
        """
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

    @jsonrpc.expose("fs/write_text_file")
    def rpc_write_text_file(self, sessionId: str, path: str, content: str) -> None:
        """Write a file in the project.

        Args:
            sessionId: The session ID.
            path: Path to the file relative to project root.
            content: Content to write.
        """
        log.info(f"[RPC] fs/write_text_file: path={path}, content_len={len(content)}")
        write_path = self.project_root / path
        log.debug(f"[RPC] fs/write_text_file: writing to {write_path}")
        write_path.parent.mkdir(parents=True, exist_ok=True)
        write_path.write_text(content, encoding="utf-8")
        log.info(
            f"[RPC] fs/write_text_file: successfully wrote {len(content)} chars to {write_path}"
        )

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
        """Agent wants to create a terminal.

        Args:
            command: Command to execute.
            _meta: Optional metadata.
            args: Command arguments.
            cwd: Working directory.
            env: Environment variables.
            outputByteLimit: Maximum output bytes to capture.
            sessionId: The session ID.

        Returns:
            Response with terminal ID.

        Raises:
            jsonrpc.JSONRPCError: If terminal creation fails.
        """
        self._terminal_count += 1
        terminal_id = f"terminal-{self._terminal_count}"

        cmd_display = command + (" " + " ".join(args) if args else "")
        log.info(f"[RPC] terminal/create: id={terminal_id}, cmd={cmd_display}")
        log.debug(f"[RPC] terminal/create: cwd={cwd}, env={env}")

        env_dict = {v["name"]: v["value"] for v in (env or [])}

        # Create and manage terminal directly
        terminal = TerminalRunner(
            terminal_id=terminal_id,
            command=command,
            args=args,
            cwd=cwd,
            env=env_dict,
            output_byte_limit=outputByteLimit,
            project_root=self.project_root,
        )
        self._terminals[terminal_id] = terminal

        # Notify UI if connected
        self.post_message(messages.AgentUpdate("terminal", f"$ {cmd_display}"))

        try:
            success = await terminal.start()
            if not success:
                log.error(f"[RPC] terminal/create: failed to start terminal {terminal_id}")
                raise jsonrpc.JSONRPCError("Failed to start terminal")
            log.info(f"[RPC] terminal/create: terminal {terminal_id} started successfully")
        except Exception as e:
            log.error(f"[RPC] terminal/create: exception starting terminal: {e}")
            del self._terminals[terminal_id]
            raise jsonrpc.JSONRPCError(f"Failed to create terminal: {e}") from e

        return {"terminalId": terminal_id}

    @jsonrpc.expose("terminal/output")
    async def rpc_terminal_output(
        self,
        sessionId: str,
        terminalId: str,
        _meta: dict[str, Any] | None = None,
    ) -> protocol.TerminalOutputResponse:
        """Get terminal output.

        Args:
            sessionId: The session ID.
            terminalId: The terminal ID.
            _meta: Optional metadata.

        Returns:
            Terminal output response.

        Raises:
            jsonrpc.JSONRPCError: If terminal not found.
        """
        terminal = self._terminals.get(terminalId)
        if terminal is None:
            raise jsonrpc.JSONRPCError(f"No terminal with id {terminalId!r}")

        state = terminal.state
        result: protocol.TerminalOutputResponse = {
            "output": state.output,
            "truncated": state.truncated,
        }
        if state.return_code is not None:
            result["exitStatus"] = {"exitCode": state.return_code}
        return result

    @jsonrpc.expose("terminal/kill")
    def rpc_terminal_kill(
        self,
        sessionId: str,
        terminalId: str,
        _meta: dict[str, Any] | None = None,
    ) -> protocol.KillTerminalCommandResponse:
        """Kill a terminal.

        Args:
            sessionId: The session ID.
            terminalId: The terminal ID.
            _meta: Optional metadata.

        Returns:
            Empty response.
        """
        if terminal := self._terminals.get(terminalId):
            terminal.kill()
        return {}

    @jsonrpc.expose("terminal/release")
    def rpc_terminal_release(
        self,
        sessionId: str,
        terminalId: str,
        _meta: dict[str, Any] | None = None,
    ) -> protocol.ReleaseTerminalResponse:
        """Release a terminal.

        Args:
            sessionId: The session ID.
            terminalId: The terminal ID.
            _meta: Optional metadata.

        Returns:
            Empty response.
        """
        if terminal := self._terminals.get(terminalId):
            terminal.kill()
            terminal.release()
        return {}

    @jsonrpc.expose("terminal/wait_for_exit")
    async def rpc_terminal_wait_for_exit(
        self,
        sessionId: str,
        terminalId: str,
        _meta: dict[str, Any] | None = None,
    ) -> protocol.WaitForTerminalExitResponse:
        """Wait for terminal to exit.

        Args:
            sessionId: The session ID.
            terminalId: The terminal ID.
            _meta: Optional metadata.

        Returns:
            Terminal exit response with exit code and signal.

        Raises:
            jsonrpc.JSONRPCError: If terminal not found.
        """
        terminal = self._terminals.get(terminalId)
        if terminal is None:
            raise jsonrpc.JSONRPCError(f"No terminal with id {terminalId!r}")

        return_code, signal = await terminal.wait_for_exit()

        # Notify UI of terminal completion
        state = terminal.state
        if state.output.strip():
            # Send last bit of output
            self.post_message(messages.AgentUpdate("terminal_output", state.output[-500:]))
        status = "success" if return_code == 0 else "error"
        self.post_message(messages.AgentUpdate("terminal_exit", f"[{status}] Exit: {return_code}"))

        return {"exitCode": return_code, "signal": signal}

    # --- ACP protocol methods ---

    async def _acp_initialize(self) -> None:
        """Initialize ACP connection."""
        log.info("[_acp_initialize] Sending initialize request to agent...")
        with self.request():
            response = api.initialize(
                PROTOCOL_VERSION,
                {"fs": {"readTextFile": True, "writeTextFile": True}, "terminal": True},
                {"name": NAME, "title": TITLE, "version": VERSION},
            )

        log.info("[_acp_initialize] Waiting for response...")
        result = await response.wait()
        log.info(f"[_acp_initialize] Received response: {result}")
        if result and (caps := result.get("agentCapabilities")):
            self.agent_capabilities = caps
            log.info(f"[_acp_initialize] Agent capabilities: {caps}")

    async def _acp_new_session(self) -> None:
        """Create a new session."""
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

    # --- Public API ---

    async def wait_ready(self, timeout: float = 30.0) -> None:
        """Wait for agent to be ready.

        Args:
            timeout: Maximum time to wait in seconds.

        Raises:
            asyncio.TimeoutError: If timeout is exceeded.
        """
        log.info(f"[wait_ready] Waiting for agent ready event (timeout={timeout}s)...")
        try:
            async with asyncio.timeout(timeout):
                await self._ready_event.wait()
            log.info("[wait_ready] Agent is ready!")
        except TimeoutError:
            log.error(f"[wait_ready] Timeout after {timeout}s waiting for agent")
            raise

    async def send_prompt(self, prompt: str) -> str | None:
        """Send a prompt to the agent.

        Args:
            prompt: The prompt text.

        Returns:
            The stop reason (e.g., "end_turn", "cancelled").
        """
        log.info(f"Sending prompt to agent (len={len(prompt)})")
        log.debug(f"Prompt content: {prompt[:500]}...")
        self._response_text.clear()
        content: list[protocol.ContentBlock] = [{"type": "text", "text": prompt}]

        with self.request():
            response = api.session_prompt(content, self.session_id)

        result = await response.wait()
        stop_reason = result.get("stopReason") if result else None
        resp_len = len(self.get_response_text())
        log.info(f"Agent response complete. stop_reason={stop_reason}, response_len={resp_len}")
        return stop_reason

    async def set_mode(self, mode_id: str) -> str | None:
        """Set the agent mode.

        Args:
            mode_id: The mode identifier.

        Returns:
            Error message or None on success.
        """
        with self.request():
            response = api.session_set_mode(self.session_id, mode_id)

        try:
            await response.wait()
        except jsonrpc.APIError as e:
            return str(e)
        return None

    async def cancel(self) -> bool:
        """Cancel current operation.

        Returns:
            True if cancellation was sent.
        """
        with self.request():
            api.session_cancel(self.session_id, {})
        return True

    async def stop(self) -> None:
        """Gracefully stop the agent."""
        if self._process and self._process.returncode is None:
            try:
                self._process.terminate()
                await asyncio.wait_for(self._process.wait(), timeout=5.0)
            except TimeoutError:
                self._process.kill()
            except ProcessLookupError:
                pass  # Process already exited

    def get_response_text(self) -> str:
        """Get accumulated response text.

        Returns:
            All response text joined together.
        """
        return "".join(self._response_text)
