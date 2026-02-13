"""Session manager for PAIR task workflows."""

from __future__ import annotations

import asyncio
import contextlib
import json
import logging
from typing import TYPE_CHECKING, Protocol

from kagan.core.adapters.process import spawn_exec
from kagan.core.config import get_os_value
from kagan.core.mcp_naming import get_mcp_server_name
from kagan.core.models.enums import (
    PairTerminalBackend,
    SessionStatus,
    SessionType,
    resolve_pair_backend,
)
from kagan.core.services.session_bundle import (
    build_external_launcher_command,
    bundle_dir,
    bundle_json_path,
    write_startup_bundle,
)
from kagan.core.tmux import TmuxError, run_tmux
from kagan.core.utils import BackgroundTasks

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.core.config import AgentConfig, KaganConfig
    from kagan.core.services.tasks import TaskService
    from kagan.core.services.types import TaskLike
    from kagan.core.services.workspaces import WorkspaceService

log = logging.getLogger(__name__)

_EXTERNAL_PAIR_BACKENDS: frozenset[str] = frozenset(
    {PairTerminalBackend.VSCODE.value, PairTerminalBackend.CURSOR.value}
)
_AGENT_MODEL_CONFIG_KEY: dict[str, str] = {
    "claude": "default_model_claude",
    "opencode": "default_model_opencode",
    "codex": "default_model_codex",
    "gemini": "default_model_gemini",
    "kimi": "default_model_kimi",
    "copilot": "default_model_copilot",
}


def _build_session_mcp_args(task_id: str, capability: str = "pair_worker") -> list[str]:
    """Build MCP CLI args with session scoping and endpoint discovery.

    Propagates ``--endpoint`` from the running core so that external ACP
    agents and the local TUI can operate concurrently.
    """
    from kagan.core.ipc.discovery import discover_core_endpoint

    session_id = task_id if task_id.startswith("task:") else f"task:{task_id}"
    args = [
        "mcp",
        "--session-id",
        session_id,
        "--capability",
        capability,
        "--identity",
        "kagan",
    ]

    endpoint = discover_core_endpoint()
    if endpoint is not None:
        if endpoint.port is not None:
            args.extend(["--endpoint", f"{endpoint.address}:{endpoint.port}"])
        else:
            args.extend(["--endpoint", endpoint.address])

    return args


class SessionService(Protocol):
    """Protocol boundary for PAIR session lifecycle operations."""

    async def create_session(self, task: TaskLike, worktree_path: Path) -> str: ...

    async def create_resolution_session(self, task: TaskLike, workdir: Path) -> str: ...

    async def attach_session(self, task_id: str) -> bool: ...

    async def attach_resolution_session(self, task_id: str) -> bool: ...

    async def session_exists(self, task_id: str) -> bool: ...

    async def resolution_session_exists(self, task_id: str) -> bool: ...

    async def kill_session(self, task_id: str) -> None: ...

    async def kill_resolution_session(self, task_id: str) -> None: ...

    async def shutdown(self) -> None: ...


class SessionServiceImpl:
    """Manages session launchers for PAIR tasks."""

    def __init__(
        self,
        project_root: Path,
        task_service: TaskService,
        workspace_service: WorkspaceService,
        config: KaganConfig,
    ) -> None:
        self._root = project_root
        self._tasks = task_service
        self._workspaces = workspace_service
        self._config = config
        self._launched_external: set[str] = set()
        self._external_proc: asyncio.subprocess.Process | None = None
        self._background_tasks = BackgroundTasks()

    def _resolve_terminal_backend(self, task: TaskLike | None) -> str:
        missing = object()
        raw_task_backend = missing if task is None else getattr(task, "terminal_backend", missing)
        config_backend = getattr(self._config.general, "default_pair_terminal_backend", None)
        return resolve_pair_backend(raw_task_backend, config_backend)

    async def _resolve_terminal_backend_for_task_id(self, task_id: str) -> str:
        task = await self._tasks.get_task(task_id)
        return self._resolve_terminal_backend(task)

    @staticmethod
    def _session_type_for_backend(backend: str) -> SessionType:
        if backend == PairTerminalBackend.TMUX.value:
            return SessionType.TMUX
        return SessionType.SCRIPT

    async def _launch_external_launcher(
        self,
        backend: str,
        worktree_path: Path,
    ) -> bool:
        cmd = build_external_launcher_command(backend, worktree_path)
        try:
            proc = await spawn_exec(
                *cmd,
                cwd=str(worktree_path),
                stdout=asyncio.subprocess.DEVNULL,
                stderr=asyncio.subprocess.DEVNULL,
            )
        except OSError:
            log.warning("Failed to start external launcher %s in %s", backend, worktree_path)
            return False
        # Fire-and-forget: the `code`/`cursor` CLI on Windows blocks until the
        # window loads — we don't need to wait.  Keep the reference so the GC
        # doesn't reap the process and trigger ResourceWarning.
        self._external_proc = proc
        return True

    def _model_for_agent(self, agent_config: AgentConfig) -> str | None:
        key = _AGENT_MODEL_CONFIG_KEY.get(agent_config.short_name.strip().lower())
        if not key:
            return None
        value = getattr(self._config.general, key, None)
        return value if isinstance(value, str) and value.strip() else None

    @staticmethod
    def _codex_mcp_server_name() -> str:
        server_name = get_mcp_server_name()
        if server_name and all(char.isalnum() or char in {"_", "-"} for char in server_name):
            return server_name
        return "kagan"

    async def create_session(self, task: TaskLike, worktree_path: Path) -> str:
        """Create session with full context injection."""
        session_name = f"kagan-{task.id}"
        backend = self._resolve_terminal_backend(task)

        agent_config = self._get_agent_config(task)
        await self._write_context_files(worktree_path, agent_config, backend, task.id)

        startup_prompt = self._build_startup_prompt(task)
        await write_startup_bundle(
            task_id=task.id,
            worktree_path=worktree_path,
            session_name=session_name,
            backend=backend,
            startup_prompt=startup_prompt,
        )
        launch_cmd = self._build_launch_command(
            agent_config,
            startup_prompt,
            self._model_for_agent(agent_config),
            task_id=task.id,
            worktree_path=worktree_path,
        )
        if backend == PairTerminalBackend.TMUX.value:
            await run_tmux(
                "new-session",
                "-d",
                "-s",
                session_name,
                "-c",
                str(worktree_path),
                "-e",
                f"KAGAN_TASK_ID={task.id}",
                "-e",
                f"KAGAN_TASK_TITLE={task.title}",
                "-e",
                f"KAGAN_WORKTREE_PATH={worktree_path}",
                "-e",
                f"KAGAN_PROJECT_ROOT={self._root}",
            )
            if launch_cmd:
                await run_tmux("send-keys", "-t", session_name, launch_cmd, "Enter")
        else:
            if not await self._launch_external_launcher(backend, worktree_path):
                raise RuntimeError(f"Failed to launch external PAIR session for task {task.id}")
            self._launched_external.add(task.id)

        workspaces = await self._workspaces.list_workspaces(task_id=task.id)
        if not workspaces:
            raise RuntimeError(f"No workspace found for task {task.id}")
        await self._tasks.create_session_record(
            workspace_id=workspaces[0].id,
            session_type=self._session_type_for_backend(backend),
            external_id=session_name,
        )

        return session_name

    def _resolve_session_name(self, task_id: str) -> str:
        return f"kagan-resolve-{task_id}"

    async def create_resolution_session(self, task: TaskLike, workdir: Path) -> str:
        """Create session for manual conflict resolution."""
        session_name = self._resolve_session_name(task.id)
        backend = self._resolve_terminal_backend(task)

        await run_tmux(
            "new-session",
            "-d",
            "-s",
            session_name,
            "-c",
            str(workdir),
            "-e",
            f"KAGAN_TASK_ID={task.id}",
            "-e",
            f"KAGAN_TASK_TITLE={task.title}",
            "-e",
            f"KAGAN_WORKTREE_PATH={workdir}",
            "-e",
            f"KAGAN_PROJECT_ROOT={self._root}",
        )

        await run_tmux("send-keys", "-t", session_name, "git status", "Enter")
        await run_tmux("send-keys", "-t", session_name, "git diff", "Enter")
        workspaces = await self._workspaces.list_workspaces(task_id=task.id)
        if not workspaces:
            raise RuntimeError(f"No workspace found for task {task.id}")
        await self._tasks.create_session_record(
            workspace_id=workspaces[0].id,
            session_type=self._session_type_for_backend(backend),
            external_id=session_name,
        )
        return session_name

    def _get_agent_config(self, task: TaskLike) -> AgentConfig:
        """Get agent config for task."""
        return task.get_agent_config(self._config)

    def _build_launch_command(
        self,
        agent_config: AgentConfig,
        prompt: str,
        model: str | None = None,
        *,
        task_id: str | None = None,
        worktree_path: Path | None = None,
    ) -> str | None:
        """Build CLI launch command with prompt for the agent.

        Args:
            agent_config: The agent configuration
            prompt: The startup prompt to send
            model: Optional model override to pass via --model flag

        Returns:
            The command string to execute, or None if no interactive command
        """
        import shlex

        from kagan.core.command_utils import is_windows

        base_cmd = get_os_value(agent_config.interactive_command)
        if not base_cmd:
            return None

        if is_windows():
            try:
                import mslex

                quote_value = mslex.quote
            except Exception:  # quality-allow-broad-except
                quote_value = shlex.quote
        else:
            quote_value = shlex.quote

        escaped_prompt = quote_value(prompt)
        model_flag = f"--model {model} " if model else ""

        match agent_config.short_name:
            case "claude":
                return f"{base_cmd} {model_flag}{escaped_prompt}"
            case "opencode":
                return f"{base_cmd} {model_flag}--prompt {escaped_prompt}"
            case "kimi":
                mcp_flag = ""
                if task_id and worktree_path is not None:
                    kimi_mcp_path = bundle_dir(worktree_path) / "kimi-mcp.json"
                    mcp_flag = f"--mcp-config-file {quote_value(str(kimi_mcp_path))} "
                return f"{base_cmd} {model_flag}{mcp_flag}--prompt {escaped_prompt}"
            case "copilot":
                # `copilot --prompt` runs one-shot (non-interactive), so keep PAIR mode interactive.
                return base_cmd
            case "codex":
                mcp_override_flags = ""
                if task_id:
                    mcp_args = _build_session_mcp_args(task_id)
                    server_name = self._codex_mcp_server_name()
                    codex_overrides = [
                        f'mcp_servers.{server_name}.command="kagan"',
                        f"mcp_servers.{server_name}.args={json.dumps(mcp_args)}",
                        f"mcp_servers.{server_name}.enabled=true",
                    ]
                    mcp_override_flags = "".join(
                        f"-c {quote_value(override)} " for override in codex_overrides
                    )
                return f"{base_cmd} {model_flag}{mcp_override_flags}{escaped_prompt}"
            case "gemini":
                return f"{base_cmd} {model_flag}{escaped_prompt}"
            case _:
                return base_cmd

    async def _attach_tmux_session(self, session_name: str) -> bool:
        """Attach to a tmux session, returning True if attach succeeded."""
        log.debug("Attaching to tmux session: %s", session_name)
        proc = await spawn_exec("tmux", "attach-session", "-t", session_name)
        returncode = await proc.wait()
        if returncode != 0:
            log.warning(
                "Failed to attach to session %s (exit code: %d)",
                session_name,
                returncode,
            )
            return False
        log.debug("Detached from session: %s", session_name)
        return True

    async def attach_session(self, task_id: str) -> bool:
        """Attach to session (blocks until detach, then returns to TUI)."""
        session_name = f"kagan-{task_id}"
        backend = await self._resolve_terminal_backend_for_task_id(task_id)
        try:
            if backend == PairTerminalBackend.TMUX.value:
                return await self._attach_tmux_session(session_name)
            worktree_path = await self._workspaces.get_path(task_id)
            if worktree_path is None:
                return False
            if task_id in self._launched_external:
                self._launched_external.discard(task_id)
                return True
            return await self._launch_external_launcher(backend, worktree_path)
        except RuntimeError:
            return False

    async def attach_resolution_session(self, task_id: str) -> bool:
        """Attach to resolution session (blocks until detach)."""
        session_name = self._resolve_session_name(task_id)
        return await self._attach_tmux_session(session_name)

    async def session_exists(self, task_id: str) -> bool:
        """Check if session exists."""
        session_name = f"kagan-{task_id}"
        backend = await self._resolve_terminal_backend_for_task_id(task_id)
        if backend in _EXTERNAL_PAIR_BACKENDS:
            worktree_path = await self._workspaces.get_path(task_id)
            if worktree_path is None:
                return False
            return bundle_json_path(worktree_path).exists()
        with contextlib.suppress(TmuxError):
            output = await run_tmux("list-sessions", "-F", "#{session_name}")
            return session_name in output.split("\n")
        return False

    async def resolution_session_exists(self, task_id: str) -> bool:
        """Check if resolution session exists."""
        session_name = self._resolve_session_name(task_id)
        with contextlib.suppress(TmuxError):
            output = await run_tmux("list-sessions", "-F", "#{session_name}")
            return session_name in output.split("\n")
        return False

    async def kill_session(self, task_id: str) -> None:
        """Kill session and mark inactive."""
        session_name = f"kagan-{task_id}"
        backend = await self._resolve_terminal_backend_for_task_id(task_id)
        if backend == PairTerminalBackend.TMUX.value:
            with contextlib.suppress(TmuxError):
                await run_tmux("kill-session", "-t", session_name)
        await self._tasks.close_session_by_external_id(session_name, status=SessionStatus.CLOSED)
        self._launched_external.discard(task_id)

    async def kill_resolution_session(self, task_id: str) -> None:
        """Kill resolution session if present."""
        session_name = self._resolve_session_name(task_id)
        with contextlib.suppress(TmuxError):
            await run_tmux("kill-session", "-t", session_name)
        await self._tasks.close_session_by_external_id(
            session_name,
            status=SessionStatus.CLOSED,
        )

    async def shutdown(self) -> None:
        """Stop any background tasks started by the session service."""
        await self._background_tasks.shutdown()

    async def _write_context_files(
        self,
        worktree_path: Path,
        agent_config: AgentConfig,
        backend: str,
        task_id: str,
    ) -> None:
        """Create MCP configuration in worktree (merging if file exists).

        Note: We no longer create CLAUDE.md, AGENTS.md, or CONTEXT.md because:
        - CLAUDE.md/AGENTS.md: Already present in worktree from git clone
        - CONTEXT.md: Redundant with MCP task_get(mode="context") tool
        """
        ignore_entries: list[str] = [".kagan/"]

        mcp_file = await self._write_mcp_config(worktree_path, agent_config, task_id)
        if mcp_file:
            ignore_entries.append(mcp_file)

        gemini_mcp_file = await self._write_gemini_mcp_config(worktree_path, agent_config, task_id)
        if gemini_mcp_file:
            ignore_entries.append(gemini_mcp_file)

        await self._write_kimi_mcp_config(worktree_path, agent_config, task_id)

        ignore_entries.extend(await self._write_ide_mcp_configs(worktree_path, backend, task_id))

        if ignore_entries:
            modified = await self._ensure_worktree_gitignored(worktree_path, ignore_entries)
            await self._commit_gitignore_if_needed(worktree_path, modified)

    async def _write_ide_mcp_configs(
        self, worktree_path: Path, backend: str, task_id: str
    ) -> list[str]:
        """Write MCP config files used by redirected IDE launchers."""
        if backend not in _EXTERNAL_PAIR_BACKENDS:
            return []

        server_name = get_mcp_server_name()
        mcp_args = _build_session_mcp_args(task_id)
        files_written: list[str] = []

        if backend == PairTerminalBackend.VSCODE.value:
            vscode_entry = {
                "type": "stdio",
                "command": "kagan",
                "args": mcp_args,
            }
            await self._merge_json_config(
                worktree_path / ".vscode" / "mcp.json",
                "servers",
                server_name,
                vscode_entry,
            )
            files_written.append(".vscode/mcp.json")

        if backend == PairTerminalBackend.CURSOR.value:
            cursor_entry = {
                "command": "kagan",
                "args": mcp_args,
            }
            await self._merge_json_config(
                worktree_path / ".cursor" / "mcp.json",
                "mcpServers",
                server_name,
                cursor_entry,
            )
            files_written.append(".cursor/mcp.json")

        return files_written

    async def _merge_json_config(
        self,
        config_path: Path,
        key: str,
        server_name: str,
        entry: dict[str, list[str] | str],
    ) -> None:
        import aiofiles

        config_path.parent.mkdir(parents=True, exist_ok=True)

        existing: dict[str, dict[str, dict[str, list[str] | str]]] = {}
        if config_path.exists():
            try:
                async with aiofiles.open(config_path, encoding="utf-8") as f:
                    existing = json.loads(await f.read())
            except json.JSONDecodeError:
                existing = {}
        if key not in existing or not isinstance(existing[key], dict):
            existing[key] = {}
        existing[key][server_name] = entry

        async with aiofiles.open(config_path, "w", encoding="utf-8") as f:
            await f.write(json.dumps(existing, indent=2))

    async def _write_mcp_config(
        self, worktree_path: Path, agent_config: AgentConfig, task_id: str
    ) -> str | None:
        """Write/merge MCP config based on agent type. Returns filename written or None."""
        import aiofiles

        from kagan.core.builtin_agents import get_builtin_agent

        builtin = get_builtin_agent(agent_config.short_name)

        if agent_config.short_name in {"codex", "gemini", "kimi"}:
            return None

        server_name = get_mcp_server_name()
        mcp_args = _build_session_mcp_args(task_id)

        if builtin and builtin.mcp_config_format == "opencode":
            filename = "opencode.json"
            kagan_entry = {
                "type": "local",
                "command": ["kagan", *mcp_args],
                "enabled": True,
            }
            mcp_key = "mcp"
        else:
            filename = ".mcp.json"
            kagan_entry = {
                "command": "kagan",
                "args": mcp_args,
            }
            mcp_key = "mcpServers"

        config_path = worktree_path / filename

        if config_path.exists():
            try:
                async with aiofiles.open(config_path, encoding="utf-8") as f:
                    content = await f.read()
                existing = json.loads(content)
            except json.JSONDecodeError:
                existing = {}
            if mcp_key not in existing:
                existing[mcp_key] = {}
            existing[mcp_key][server_name] = kagan_entry
            config = existing
        else:
            config: dict[str, object] = {mcp_key: {server_name: kagan_entry}}
            if filename == "opencode.json":
                config["$schema"] = "https://opencode.ai/config.json"

        async with aiofiles.open(config_path, "w", encoding="utf-8") as f:
            await f.write(json.dumps(config, indent=2))
        return filename

    async def _write_gemini_mcp_config(
        self, worktree_path: Path, agent_config: AgentConfig, task_id: str
    ) -> str | None:
        """Write Gemini project-local MCP config for this PAIR session."""
        if agent_config.short_name != "gemini":
            return None

        server_name = get_mcp_server_name()
        mcp_args = _build_session_mcp_args(task_id)
        entry = {
            "command": "kagan",
            "args": mcp_args,
        }
        await self._merge_json_config(
            worktree_path / ".gemini" / "settings.json",
            "mcpServers",
            server_name,
            entry,
        )
        return ".gemini/settings.json"

    async def _write_kimi_mcp_config(
        self, worktree_path: Path, agent_config: AgentConfig, task_id: str
    ) -> None:
        """Write ad-hoc Kimi MCP config consumed via --mcp-config-file."""
        import aiofiles

        if agent_config.short_name != "kimi":
            return

        server_name = get_mcp_server_name()
        mcp_args = _build_session_mcp_args(task_id)
        config_path = bundle_dir(worktree_path) / "kimi-mcp.json"
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config = {"mcpServers": {server_name: {"command": "kagan", "args": mcp_args}}}

        async with aiofiles.open(config_path, "w", encoding="utf-8") as f:
            await f.write(json.dumps(config, indent=2))

    async def _ensure_worktree_gitignored(self, worktree_path: Path, entries: list[str]) -> bool:
        """Add Kagan MCP config to worktree's .gitignore. Returns True if file was modified."""
        import aiofiles

        gitignore = worktree_path / ".gitignore"
        kagan_entries = entries

        existing_content = ""
        if gitignore.exists():
            async with aiofiles.open(gitignore, encoding="utf-8") as f:
                existing_content = await f.read()
            existing_lines = set(existing_content.split("\n"))

            if all(e in existing_lines for e in kagan_entries):
                return False

        addition = "\n# Kagan MCP config (auto-generated)\n"
        addition += "\n".join(kagan_entries) + "\n"

        if existing_content and not existing_content.endswith("\n"):
            addition = "\n" + addition

        async with aiofiles.open(gitignore, "w", encoding="utf-8") as f:
            await f.write(existing_content + addition)
        return True

    async def _commit_gitignore_if_needed(self, worktree_path: Path, modified: bool) -> None:
        """Schedule .gitignore commit in background so session creation isn't blocked."""
        if not modified:
            return
        self._background_tasks.spawn(self._bg_commit_gitignore(worktree_path))

    async def _bg_commit_gitignore(self, worktree_path: Path) -> None:
        """Background task to commit .gitignore changes (best-effort)."""
        try:
            proc = await spawn_exec(
                "git",
                "-C",
                str(worktree_path),
                "add",
                ".gitignore",
                stdout=asyncio.subprocess.DEVNULL,
                stderr=asyncio.subprocess.DEVNULL,
            )
            await proc.wait()
            proc = await spawn_exec(
                "git",
                "-C",
                str(worktree_path),
                "commit",
                "-m",
                "chore: gitignore kagan mcp config files",
                stdout=asyncio.subprocess.DEVNULL,
                stderr=asyncio.subprocess.DEVNULL,
            )
            await proc.wait()
        except (FileNotFoundError, OSError):
            pass  # Best-effort background commit

    def _build_startup_prompt(self, task: TaskLike) -> str:
        """Build startup prompt for pair mode using consolidated tool names."""
        desc = task.description or "No description provided."
        server_name = get_mcp_server_name()
        tool_format_note = (
            f"Tool names vary by client. Use the Kagan MCP server tools named like "
            f"`mcp__{server_name}__<tool>` or `{server_name}_<tool>`."
        )
        return f"""Hello! I'm starting a pair programming session for task **{task.id}**.

Act as a Senior Developer collaborating with me on this implementation.

## Task Overview
**Title:** {task.title}

**Description:**
{desc}

## Important Rules
- You are in a git worktree, NOT the main repository
- Only modify files within this worktree
- **COMMIT all changes before requesting review** (use semantic commits: feat:, fix:, docs:, etc.)
- When complete: commit your work, then call `task_patch(transition='request_review')`

## MCP Tools Available
{tool_format_note}

This session uses capability profile `pair_worker` scoped to task `{task.id}`.

**Context Tools:**
- `task_get(mode="context")` - Full task details (acceptance criteria, scratchpad, linked tasks)
- `task_get` - Look up any task's details by ID (useful for @mentioned tasks)
- `task_patch(append_note=...)` - Record progress, decisions, and blockers

**Coordination Tools (USE THESE):**
- `task_list` - Discover concurrent work to avoid merge conflicts
- `task_get(task_id, include_logs=true)` - Execution logs from prior work

**Read-Only Browsing:**
- `task_list` - List tasks with optional filter/project/exclusion controls
- `project_list` - List recent projects
- `repo_list` - List repos for a project
- `audit_list` - Recent audit events

**Completion:**
- `task_patch(transition="request_review")` - Submit work for review (commit first!)

## Coordination Workflow

Before implementing, check for parallel work and historical context:

1. **Check parallel work**: Call
   `task_list(filter="IN_PROGRESS", exclude_task_ids=["{task.id}"], include_scratchpad=true)`.
   Review concurrent tasks to identify overlapping file modifications or shared dependencies.

2. **Learn from history**: Call `task_get(task_id, include_logs=true)` on related completed tasks.
   Avoid repeating failed approaches; reuse successful patterns.

3. **Coordinate strategy**: If overlap exists, plan which files to modify first or wait for.

## Setup Verification

Please confirm you have access to the Kagan MCP tools by:
1. Calling `task_get` with `mode="context"` and task_id: `{task.id}`
2. Calling
   `task_list(filter="IN_PROGRESS", exclude_task_ids=["{task.id}"], include_scratchpad=true)`

After confirming MCP access, please:
1. Summarize your understanding of this task (including acceptance criteria from MCP)
2. Report any parallel work that might affect our implementation
3. Ask me if I'm ready to proceed with the implementation

**Wait for my confirmation before beginning any implementation.**
"""
