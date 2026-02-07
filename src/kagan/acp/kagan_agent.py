"""ACP client implementation backed by the agent-client-protocol SDK."""

from __future__ import annotations

import asyncio
import contextlib
import json
import os
import shlex
from typing import TYPE_CHECKING, Any

import aiofiles
from acp import PROTOCOL_VERSION, RequestError, spawn_agent_process, text_block
from acp.schema import (
    AgentCapabilities,
    AgentMessageChunk,
    AgentPlanUpdate,
    AgentThoughtChunk,
    AllowedOutcome,
    AvailableCommandsUpdate,
    ClientCapabilities,
    CreateTerminalResponse,
    CurrentModeUpdate,
    DeniedOutcome,
    FileSystemCapability,
    Implementation,
    McpServerStdio,
    PermissionOption,
    PromptResponse,
    ReadTextFileResponse,
    ReleaseTerminalResponse,
    RequestPermissionResponse,
    SessionInfoUpdate,
    TerminalOutputResponse,
    ToolCall,
    ToolCallProgress,
    ToolCallStart,
    ToolCallUpdate,
    WaitForTerminalExitResponse,
    WriteTextFileResponse,
)

from kagan.acp import messages
from kagan.acp.buffers import AgentBuffers
from kagan.acp.terminals import TerminalManager
from kagan.debug_log import log
from kagan.limits import SHUTDOWN_TIMEOUT, SUBPROCESS_LIMIT
from kagan.mcp_naming import get_mcp_server_name

if TYPE_CHECKING:
    from pathlib import Path

    from acp.schema import EnvVariable, UserMessageChunk
    from textual.message import Message
    from textual.message_pump import MessagePump

    from kagan.config import AgentConfig

PROTOCOL_NAME = "kagan"
PROTOCOL_TITLE = "Kagan"
PROTOCOL_VERSION_NAME = "0.1.0"

_SENSITIVE_FILENAMES = {
    ".npmrc",
    ".pypirc",
    ".netrc",
    "credentials.json",
    "credentials.yaml",
    "credentials.yml",
    "secret.json",
    "secret.yaml",
    "secret.yml",
    "secrets.json",
    "secrets.yaml",
    "secrets.yml",
    "token.json",
    "tokens.json",
    "id_rsa",
    "id_rsa.pub",
    "id_ed25519",
    "id_ed25519.pub",
}

_SENSITIVE_EXTENSIONS = {
    ".pem",
    ".key",
    ".p12",
    ".pfx",
    ".crt",
    ".cer",
    ".der",
    ".jks",
    ".kdbx",
    ".gpg",
}

_SENSITIVE_DIRS = {
    ".ssh",
    ".gnupg",
    ".aws",
    ".azure",
    ".kube",
}


class KaganAgent:
    """ACP-based agent communication via the official ACP SDK."""

    def __init__(
        self, project_root: Path, agent_config: AgentConfig, *, read_only: bool = False
    ) -> None:
        self.project_root = project_root
        self._agent_config = agent_config
        self._read_only = read_only
        self._model_override: str | None = None

        self._connection = None
        self._process: asyncio.subprocess.Process | None = None
        self._agent_task: asyncio.Task[None] | None = None
        self._cleanup_task: asyncio.Task[None] | None = None

        self.session_id: str = ""
        self.tool_calls: dict[str, ToolCall] = {}
        self.agent_capabilities: AgentCapabilities = AgentCapabilities()

        self._message_target: MessagePump | None = None
        self._buffers = AgentBuffers()
        self._terminals = TerminalManager(project_root)

        self._ready_event = asyncio.Event()
        self._done_event = asyncio.Event()
        self._auto_approve = False
        self._stop_requested = False
        self._prompt_completed = False

    @property
    def command(self) -> str | None:
        from kagan import get_os_value
        from kagan.preflight import resolve_acp_command

        raw_command = get_os_value(self._agent_config.run_command)
        if raw_command is None:
            return None

        resolution = resolve_acp_command(raw_command, self._agent_config.name)
        if resolution.resolved_command is None:
            return None
        return shlex.join(resolution.resolved_command)

    def on_connect(self, conn) -> None:
        self._connection = conn

    def set_message_target(self, target: MessagePump | None) -> None:
        self._message_target = target
        if target is not None and self._buffers.messages:
            log.debug(f"Replaying {len(self._buffers.messages)} buffered messages to new target")
            self._buffers.replay_messages_to(target)

    def set_auto_approve(self, enabled: bool) -> None:
        self._auto_approve = enabled
        log.debug(f"Auto-approve mode: {enabled}")

    def set_model_override(self, model_id: str | None) -> None:
        """Set the model override for this agent session."""
        self._model_override = model_id
        log.debug(f"Model override set: {model_id}")

    def get_model_override(self) -> str | None:
        """Get the current model override."""
        return self._model_override

    def start(self, message_target: MessagePump | None = None) -> None:
        log.info(f"Starting agent for project: {self.project_root}")
        log.debug(f"Agent config: {self._agent_config}")
        self._message_target = message_target
        self._stop_requested = False
        self._prompt_completed = False
        self._ready_event.clear()
        self._done_event.clear()
        self._agent_task = asyncio.create_task(self._run_agent())

    async def _run_agent(self) -> None:
        log.info(f"[_run_agent] Starting for project: {self.project_root}")
        env = os.environ.copy()
        env["KAGAN_CWD"] = str(self.project_root.absolute())

        if self._model_override:
            if self._agent_config.model_env_var:
                env_var = self._agent_config.model_env_var
                env[env_var] = self._model_override
                log.info(f"[_run_agent] Model override: {env_var}={self._model_override}")
            elif "opencode" in self._agent_config.identity.lower():
                config_content = json.dumps({"model": self._model_override})
                env["OPENCODE_CONFIG_CONTENT"] = config_content
                log.info(f"[_run_agent] OpenCode model override: {self._model_override}")

        command = self.command
        if command is None:
            log.error("[_run_agent] No run command for this OS")
            self.post_message(messages.AgentFail("No run command for this OS"))
            return

        try:
            parts = shlex.split(command)
        except ValueError as exc:
            log.error(f"[_run_agent] Failed to parse command: {exc}")
            self.post_message(messages.AgentFail("Failed to parse run command", str(exc)))
            return

        if not parts:
            log.error("[_run_agent] Empty ACP command")
            self.post_message(messages.AgentFail("Invalid run command", "Run command is empty"))
            return

        log.info(f"[_run_agent] Spawning agent process: {command}")
        log.info(f"[_run_agent] Working directory: {self.project_root}")
        log.info(f"[_run_agent] KAGAN_CWD={env['KAGAN_CWD']}")

        try:
            async with (
                spawn_agent_process(
                    self,  # type: ignore[arg-type]
                    parts[0],
                    *parts[1:],
                    env=env,
                    cwd=str(self.project_root.absolute()),
                    transport_kwargs={
                        "limit": SUBPROCESS_LIMIT,
                        "shutdown_timeout": SHUTDOWN_TIMEOUT,
                    },
                ) as (conn, process)
            ):
                self._connection = conn
                self._process = process
                await self._initialize(conn)
                await process.wait()
                if process.returncode and not self._should_ignore_exit_code(process.returncode):
                    fail_details = await self._read_process_stderr(process)
                    log.error(
                        f"[_run_agent] Agent exited with code {process.returncode}: "
                        f"{fail_details[:500]}"
                    )
                    self.post_message(
                        messages.AgentFail(
                            f"Agent exited with code {process.returncode}",
                            fail_details,
                        )
                    )
                elif process.returncode:
                    log.info(
                        f"[_run_agent] Ignoring expected process exit code {process.returncode} "
                        f"(stop_requested={self._stop_requested}, "
                        f"prompt_completed={self._prompt_completed})"
                    )
        except RequestError as exc:
            log.error(f"[_run_agent] ACP request error: {exc}")
            self.post_message(messages.AgentFail("Failed to initialize", str(exc)))
        except Exception as exc:
            log.exception("[_run_agent] Unexpected error")
            self.post_message(messages.AgentFail("Failed to start agent", str(exc)))
        finally:
            self._done_event.set()

    async def _read_process_stderr(self, process: asyncio.subprocess.Process) -> str:
        if process.stderr is None:
            return "stderr not available"
        try:
            data = await process.stderr.read()
            return data.decode("utf-8", "replace")
        except Exception as exc:
            return f"Failed to read stderr: {exc}"

    async def _initialize(self, conn) -> None:
        log.info("[_initialize] Starting ACP handshake...")
        try:
            log.info("[_initialize] Sending initialize request...")
            await self._acp_initialize(conn)
            log.info("[_initialize] initialize complete, sending session/new...")
            await self._acp_new_session(conn)
            log.info(f"[_initialize] ACP handshake complete, session_id={self.session_id}")
            self._ready_event.set()
            self.post_message(messages.AgentReady())
        except RequestError as exc:
            log.error(f"[_initialize] ACP handshake failed: {exc}")
            self.post_message(messages.AgentFail("Failed to initialize", str(exc)))
        except Exception as exc:
            log.exception("[_initialize] Unexpected error in handshake")
            self.post_message(messages.AgentFail("Failed to initialize", str(exc)))

    def post_message(self, message: Message, buffer: bool = True) -> bool:
        if buffer and not isinstance(message, messages.RequestPermission):
            self._buffers.buffer_message(message)

        if self._message_target is not None:
            return self._message_target.post_message(message)
        return False

    async def session_update(
        self,
        session_id: str,
        update: (
            UserMessageChunk
            | AgentMessageChunk
            | AgentThoughtChunk
            | ToolCallStart
            | ToolCallProgress
            | AgentPlanUpdate
            | AvailableCommandsUpdate
            | CurrentModeUpdate
            | SessionInfoUpdate
        ),
        **kwargs: Any,
    ) -> None:
        del session_id, kwargs
        if isinstance(update, AgentMessageChunk):
            content = update.content
            content_type = content.type
            if content_type == "text":
                text = content.text
                self._buffers.append_response(text)
                self.post_message(messages.AgentUpdate(content_type, text))
        elif isinstance(update, AgentThoughtChunk):
            content = update.content
            content_type = content.type
            if content_type == "text":
                self.post_message(messages.Thinking(content_type, content.text))
        elif isinstance(update, ToolCallStart):
            self.tool_calls[update.tool_call_id] = update
            self.post_message(messages.ToolCall(update))
        elif isinstance(update, ToolCallProgress):
            tool_call = self._apply_tool_call_update(update)
            self.post_message(messages.ToolCallUpdate(tool_call, update))
        elif isinstance(update, AgentPlanUpdate):
            self.post_message(messages.Plan(update.entries))
        elif isinstance(update, AvailableCommandsUpdate):
            self.post_message(messages.AvailableCommandsUpdate(update.available_commands))
        elif isinstance(update, CurrentModeUpdate):
            self.post_message(messages.ModeUpdate(update.current_mode_id))

    async def request_permission(
        self,
        options: list[PermissionOption],
        session_id: str,
        tool_call: ToolCallUpdate,
        **kwargs: Any,
    ) -> RequestPermissionResponse:
        del session_id, kwargs
        tool_call_id = tool_call.tool_call_id
        tool_title = tool_call.title or "Unknown"
        log.info(f"[ACP] session/request_permission: tool={tool_title}, id={tool_call_id}")
        log.debug(f"[ACP] session/request_permission: options={options}")

        tool_call_record = self._apply_tool_call_update(tool_call)

        if self._auto_approve or self._message_target is None:
            reason = "auto_approve enabled" if self._auto_approve else "no UI"
            log.info(f"[ACP] session/request_permission: {reason}, auto-approving")
            option_id = _find_option_id(options, "allow_once")
            if option_id is None:
                option_id = _find_option_id(options, "allow_always")
            if option_id is None and options:
                option_id = options[0].option_id
            if option_id:
                return RequestPermissionResponse(
                    outcome=AllowedOutcome(outcome="selected", optionId=option_id)
                )
            return RequestPermissionResponse(outcome=DeniedOutcome(outcome="cancelled"))

        log.info("[ACP] session/request_permission: waiting for UI response")
        result_future: asyncio.Future[messages.Answer] = asyncio.Future()
        self.post_message(messages.RequestPermission(options, tool_call_record, result_future))

        try:
            answer = await asyncio.wait_for(result_future, timeout=330.0)
        except TimeoutError:
            log.warning("[ACP] session/request_permission: timeout, auto-rejecting")
            option_id = _find_option_id(options, "reject_once")
            if option_id is None:
                option_id = _find_option_id(options, "reject_always")
            if option_id is None and options:
                option_id = options[0].option_id
            if option_id:
                return RequestPermissionResponse(
                    outcome=AllowedOutcome(outcome="selected", optionId=option_id)
                )
            return RequestPermissionResponse(outcome=DeniedOutcome(outcome="cancelled"))

        log.info(f"[ACP] session/request_permission: UI responded with {answer.id}")
        if not answer.id:
            return RequestPermissionResponse(outcome=DeniedOutcome(outcome="cancelled"))
        return RequestPermissionResponse(
            outcome=AllowedOutcome(outcome="selected", optionId=answer.id)
        )

    async def read_text_file(
        self,
        path: str,
        session_id: str,
        limit: int | None = None,
        line: int | None = None,
        **kwargs: Any,
    ) -> ReadTextFileResponse:
        del session_id, kwargs
        log.info(f"[ACP] fs/read_text_file: path={path}, line={line}, limit={limit}")
        read_path = self._resolve_read_path(path)
        if read_path is None:
            raise RequestError.invalid_params(
                {"details": "Access denied: path outside project root"}
            )
        if self._is_sensitive_path(read_path):
            log.warning(f"[ACP] fs/read_text_file: BLOCKED sensitive path {read_path}")
            raise RequestError.invalid_params({"details": "Access denied: sensitive file"})
        try:
            async with aiofiles.open(read_path, encoding="utf-8", errors="ignore") as handle:
                text = await handle.read()
            log.debug(f"[ACP] fs/read_text_file: read {len(text)} chars from {read_path}")
        except OSError as exc:
            log.warning(f"[ACP] fs/read_text_file: failed to read {read_path}: {exc}")
            text = ""

        if line is not None:
            line = max(0, line - 1)
            lines = text.splitlines()
            text = (
                "\n".join(lines[line:]) if limit is None else "\n".join(lines[line : line + limit])
            )

        return ReadTextFileResponse(content=text)

    async def write_text_file(
        self,
        content: str,
        path: str,
        session_id: str,
        **kwargs: Any,
    ) -> WriteTextFileResponse | None:
        del session_id, kwargs
        if self._read_only:
            log.warning(f"[ACP] fs/write_text_file: BLOCKED in read-only mode (path={path})")
            raise RequestError.invalid_params(
                {"details": "Write operations not permitted in read-only mode"}
            )

        log.info(f"[ACP] fs/write_text_file: path={path}, content_len={len(content)}")
        write_path = self._resolve_write_path(path)
        if write_path is None:
            raise RequestError.invalid_params(
                {"details": "Access denied: path outside project root"}
            )
        log.debug(f"[ACP] fs/write_text_file: writing to {write_path}")
        await asyncio.to_thread(write_path.parent.mkdir, parents=True, exist_ok=True)
        async with aiofiles.open(write_path, "w", encoding="utf-8") as handle:
            await handle.write(content)
        log.info(
            f"[ACP] fs/write_text_file: successfully wrote {len(content)} chars to {write_path}"
        )
        return WriteTextFileResponse()

    async def create_terminal(
        self,
        command: str,
        session_id: str,
        args: list[str] | None = None,
        cwd: str | None = None,
        env: list[EnvVariable] | None = None,
        output_byte_limit: int | None = None,
        **kwargs: Any,
    ) -> CreateTerminalResponse:
        del session_id, kwargs
        if self._read_only:
            log.warning(f"[ACP] terminal/create: BLOCKED in read-only mode (command={command})")
            raise RequestError.invalid_params(
                {"details": "Terminal operations not permitted in read-only mode"}
            )
        if self._command_mentions_sensitive(command, args):
            log.warning(f"[ACP] terminal/create: BLOCKED sensitive command {command}")
            raise RequestError.invalid_params(
                {"details": "Terminal command blocked: references sensitive files"}
            )

        terminal_id, cmd_display = await self._terminals.create(
            command=command,
            args=args,
            cwd=cwd,
            env=env,
            output_byte_limit=output_byte_limit,
        )
        self.post_message(messages.AgentUpdate("terminal", f"$ {cmd_display}"))
        return CreateTerminalResponse(terminalId=terminal_id)

    async def terminal_output(
        self,
        session_id: str,
        terminal_id: str,
        **kwargs: Any,
    ) -> TerminalOutputResponse:
        del session_id, kwargs
        return self._terminals.get_output(terminal_id)

    async def kill_terminal(
        self,
        session_id: str,
        terminal_id: str,
        **kwargs: Any,
    ) -> None:
        del session_id, kwargs
        self._terminals.kill(terminal_id)
        return None

    async def release_terminal(
        self,
        session_id: str,
        terminal_id: str,
        **kwargs: Any,
    ) -> ReleaseTerminalResponse | None:
        del session_id, kwargs
        self._terminals.release(terminal_id)
        return ReleaseTerminalResponse()

    async def wait_for_terminal_exit(
        self,
        session_id: str,
        terminal_id: str,
        **kwargs: Any,
    ) -> WaitForTerminalExitResponse:
        del session_id, kwargs
        return_code, signal = await self._terminals.wait_for_exit(terminal_id)

        final_output = self._terminals.get_final_output(terminal_id)
        if final_output.strip():
            self.post_message(messages.AgentUpdate("terminal_output", final_output))
        status = "success" if return_code == 0 else "error"
        self.post_message(messages.AgentUpdate("terminal_exit", f"[{status}] Exit: {return_code}"))

        return WaitForTerminalExitResponse(exit_code=return_code, signal=signal)

    def _apply_tool_call_update(self, update: ToolCallUpdate) -> ToolCall:
        tool_call_id = update.tool_call_id
        existing = self.tool_calls.get(tool_call_id)
        if existing is None:
            title = update.title or "Tool call"
            existing = ToolCall(toolCallId=tool_call_id, title=title)
            self.tool_calls[tool_call_id] = existing

        if update.title is not None:
            existing.title = update.title
        if update.kind is not None:
            existing.kind = update.kind
        if update.status is not None:
            existing.status = update.status
        if update.content is not None:
            existing.content = update.content
        if update.locations is not None:
            existing.locations = update.locations
        if update.raw_input is not None:
            existing.raw_input = update.raw_input
        if update.raw_output is not None:
            existing.raw_output = update.raw_output
        return existing

    def _resolve_read_path(self, path: str) -> Path | None:
        project_root = self.project_root.resolve()
        read_path = self._resolve_path(path)
        if read_path is None:
            return None
        if not read_path.is_relative_to(project_root):
            log.warning(f"[ACP] fs/read_text_file: BLOCKED path traversal {read_path}")
            return None
        return read_path

    def _resolve_write_path(self, path: str) -> Path | None:
        project_root = self.project_root.resolve()
        write_path = self._resolve_path(path)
        if write_path is None:
            return None
        if not write_path.is_relative_to(project_root):
            log.warning(f"[ACP] fs/write_text_file: BLOCKED path traversal {write_path}")
            return None
        return write_path

    def _resolve_path(self, path: str) -> Path | None:
        from pathlib import Path

        candidate = Path(path)
        if not candidate.is_absolute():
            candidate = self.project_root / candidate
        return candidate.resolve()

    def _is_sensitive_path(self, path: Path) -> bool:
        name = path.name.lower()
        if name == ".env" or name.startswith(".env."):
            return True
        if name in _SENSITIVE_FILENAMES:
            return True
        if path.suffix.lower() in _SENSITIVE_EXTENSIONS:
            return True
        return any(part.lower() in _SENSITIVE_DIRS for part in path.parts)

    def _command_mentions_sensitive(self, command: str, args: list[str] | None) -> bool:
        tokens = [command] + (args or [])
        haystack = " ".join(tokens).lower()
        sensitive_markers = (
            ".env",
            "id_rsa",
            "id_ed25519",
            "credentials.json",
            "credentials.yml",
            "credentials.yaml",
            "secrets.json",
            "secrets.yml",
            "secrets.yaml",
            "secret.json",
            "secret.yml",
            "secret.yaml",
            ".pem",
            ".key",
            ".p12",
            ".pfx",
            ".crt",
            ".cer",
            ".der",
            ".jks",
            ".kdbx",
            ".gpg",
        )
        return any(marker in haystack for marker in sensitive_markers)

    async def _acp_initialize(self, conn) -> None:
        fs_caps = FileSystemCapability(read_text_file=True, write_text_file=not self._read_only)
        client_caps = ClientCapabilities(fs=fs_caps, terminal=not self._read_only)
        log.info(f"[_acp_initialize] Capabilities: read_only={self._read_only}, caps={client_caps}")

        result = await conn.initialize(
            protocol_version=PROTOCOL_VERSION,
            client_capabilities=client_caps,
            client_info=Implementation(
                name=PROTOCOL_NAME,
                title=PROTOCOL_TITLE,
                version=PROTOCOL_VERSION_NAME,
            ),
        )
        if result.agent_capabilities:
            self.agent_capabilities = result.agent_capabilities
            log.info(f"[_acp_initialize] Agent capabilities: {result.agent_capabilities}")

    async def _acp_new_session(self, conn) -> None:
        cwd = str(self.project_root.absolute())
        log.info(f"[_acp_new_session] Sending session/new request with cwd={cwd}")

        kagan_mcp = McpServerStdio(
            name=get_mcp_server_name(),
            command="kagan",
            args=["mcp", "--readonly"],
            env=[],
        )

        result = await conn.new_session(cwd=cwd, mcp_servers=[kagan_mcp])
        self.session_id = result.session_id
        log.info(f"[_acp_new_session] Session created: {self.session_id}")

        if result.modes:
            current_mode = result.modes.current_mode_id
            modes_dict = {
                mode.id: messages.Mode(mode.id, mode.name, mode.description)
                for mode in result.modes.available_modes
            }
            self.post_message(messages.SetModes(current_mode, modes_dict))

    async def wait_ready(self, timeout: float = 30.0) -> None:
        log.info(f"[wait_ready] Waiting for agent ready event (timeout={timeout}s)...")
        try:
            async with asyncio.timeout(timeout):
                await self._ready_event.wait()
            log.info("[wait_ready] Agent is ready!")
        except TimeoutError:
            log.error(f"[wait_ready] Timeout after {timeout}s waiting for agent")
            raise

    def clear_tool_calls(self) -> None:
        """Clear accumulated tool calls."""
        self.tool_calls.clear()

    async def send_prompt(self, prompt: str) -> str | None:
        log.info(f"Sending prompt to agent (len={len(prompt)})")
        log.debug(f"Prompt content: {prompt[:500]}...")
        self._buffers.clear_response()
        self.tool_calls.clear()

        if self._connection is None:
            raise RequestError.internal_error({"details": "Agent connection not ready"})

        try:
            result: PromptResponse = await self._connection.prompt(
                prompt=[text_block(prompt)],
                session_id=self.session_id,
            )
        except RequestError as exc:
            log.error(f"[send_prompt] ACP error: {exc}")
            error_details = ""
            if exc.data and isinstance(exc.data, dict):
                error_details = str(exc.data.get("details") or exc.data.get("error") or "")
            self.post_message(messages.AgentFail(f"Agent error: {exc}", error_details))
            raise

        stop_reason = result.stop_reason if result else None
        resp_len = len(self.get_response_text())
        log.info(f"Agent response complete. stop_reason={stop_reason}, response_len={resp_len}")
        self._prompt_completed = True
        self.post_message(messages.AgentComplete())
        return str(stop_reason) if stop_reason is not None else None

    async def set_mode(self, mode_id: str) -> str | None:
        if self._connection is None:
            return "Agent connection not ready"
        try:
            await self._connection.set_session_mode(session_id=self.session_id, mode_id=mode_id)
        except RequestError as exc:
            return str(exc)
        return None

    async def cancel(self) -> bool:
        if self._connection is None:
            return False
        await self._connection.cancel(session_id=self.session_id)
        return True

    async def _background_cleanup(self) -> None:
        if self._process is None:
            return
        try:
            await asyncio.wait_for(self._process.wait(), timeout=SHUTDOWN_TIMEOUT)
        except TimeoutError:
            with contextlib.suppress(ProcessLookupError):
                self._process.kill()
        except ProcessLookupError:
            pass
        if self._connection is not None:
            with contextlib.suppress(Exception):
                await self._connection.close()

    async def stop(self) -> None:
        """Stop the agent process gracefully (non-blocking)."""
        self._terminals.cleanup_all()
        self._buffers.clear_all()
        self._stop_requested = True

        if self._process and self._process.returncode is None:
            self._process.terminate()
            self._cleanup_task = asyncio.create_task(self._background_cleanup())

    def get_response_text(self) -> str:
        return self._buffers.get_response_text()

    def _should_ignore_exit_code(self, code: int) -> bool:
        """Treat expected process termination as non-errors."""
        return code == -15 and (self._stop_requested or self._prompt_completed)


def _find_option_id(options: list[PermissionOption], kind: str) -> str | None:
    for option in options:
        if option.kind == kind:
            return option.option_id
    return None
