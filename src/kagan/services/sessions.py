"""Session manager for PAIR task workflows."""

from __future__ import annotations

import asyncio
import contextlib
import json
import logging
from typing import TYPE_CHECKING

from kagan.config import get_os_value
from kagan.core.models.enums import PairTerminalBackend, SessionStatus, SessionType
from kagan.mcp_naming import get_mcp_server_name
from kagan.tmux import TmuxError, run_tmux
from kagan.wezterm import WeztermError, create_workspace_session, kill_workspace, run_wezterm
from kagan.wezterm import workspace_exists as wezterm_workspace_exists

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.config import AgentConfig, KaganConfig
    from kagan.services.tasks import TaskService
    from kagan.services.types import TaskLike
    from kagan.services.workspaces import WorkspaceService

log = logging.getLogger(__name__)

_PAIR_BACKENDS = {"tmux", "vscode", "cursor"}
_EXTERNAL_PAIR_BACKENDS = {"vscode", "cursor"}
_SESSION_BUNDLE_DIR = ".kagan"
_SESSION_BUNDLE_JSON = "session.json"
_SESSION_BUNDLE_PROMPT = "start_prompt.md"
_AGENT_MODEL_CONFIG_KEY: dict[str, str] = {
    "claude": "default_model_claude",
    "opencode": "default_model_opencode",
    "codex": "default_model_codex",
    "gemini": "default_model_gemini",
    "kimi": "default_model_kimi",
    "copilot": "default_model_copilot",
}


class SessionService:
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

    @staticmethod
    def _coerce_terminal_backend(value: object) -> str | None:
        if isinstance(value, PairTerminalBackend):
            return value.value
        if isinstance(value, str):
            normalized = value.strip().lower()
            if normalized in _PAIR_BACKENDS:
                return normalized
        return None

    def _resolve_terminal_backend(self, task: TaskLike | None) -> str:
        missing = object()
        raw_task_backend = missing if task is None else getattr(task, "terminal_backend", missing)
        task_backend = self._coerce_terminal_backend(raw_task_backend)
        if task_backend is not None:
            return task_backend

        default_backend = self._coerce_terminal_backend(
            getattr(self._config.general, "default_pair_terminal_backend", None)
        )
        if default_backend is not None:
            return default_backend

        return PairTerminalBackend.TMUX.value

    async def _resolve_terminal_backend_for_task_id(self, task_id: str) -> str:
        task = await self._tasks.get_task(task_id)
        return self._resolve_terminal_backend(task)

    @staticmethod
    def _session_type_for_backend(backend: str) -> SessionType:
        if backend == PairTerminalBackend.WEZTERM.value:
            return SessionType.WEZTERM
        if backend == PairTerminalBackend.TMUX.value:
            return SessionType.TMUX
        return SessionType.SCRIPT

    @staticmethod
    def _bundle_dir(worktree_path: Path) -> Path:
        return worktree_path / _SESSION_BUNDLE_DIR

    @classmethod
    def _bundle_prompt_path(cls, worktree_path: Path) -> Path:
        return cls._bundle_dir(worktree_path) / _SESSION_BUNDLE_PROMPT

    @classmethod
    def _bundle_json_path(cls, worktree_path: Path) -> Path:
        return cls._bundle_dir(worktree_path) / _SESSION_BUNDLE_JSON

    async def _write_startup_bundle(
        self,
        task: TaskLike,
        worktree_path: Path,
        session_name: str,
        backend: str,
        startup_prompt: str,
    ) -> None:
        bundle_dir = self._bundle_dir(worktree_path)
        await asyncio.to_thread(bundle_dir.mkdir, parents=True, exist_ok=True)

        prompt_file = self._bundle_prompt_path(worktree_path)
        await asyncio.to_thread(prompt_file.write_text, startup_prompt, "utf-8")

        session_file = self._bundle_json_path(worktree_path)
        payload = {
            "task_id": task.id,
            "session_name": session_name,
            "backend": backend,
            "worktree": str(worktree_path),
            "prompt_file": str(prompt_file),
        }
        await asyncio.to_thread(session_file.write_text, json.dumps(payload, indent=2), "utf-8")

    def _build_external_launcher_command(self, backend: str, worktree_path: Path) -> list[str]:
        prompt_file = self._bundle_prompt_path(worktree_path)
        if backend in {"vscode", "cursor"}:
            binary = {"vscode": "code", "cursor": "cursor"}[backend]
            return [binary, "--new-window", str(worktree_path), str(prompt_file)]

        raise RuntimeError(f"Unsupported external PAIR launcher: {backend}")

    async def _launch_external_launcher(
        self,
        backend: str,
        worktree_path: Path,
    ) -> bool:
        cmd = self._build_external_launcher_command(backend, worktree_path)
        try:
            proc = await asyncio.create_subprocess_exec(
                *cmd,
                cwd=str(worktree_path),
                stdout=asyncio.subprocess.DEVNULL,
                stderr=asyncio.subprocess.DEVNULL,
            )
        except OSError:
            log.warning("Failed to start external launcher %s in %s", backend, worktree_path)
            return False
        return await proc.wait() == 0

    @staticmethod
    def _build_session_env(task: TaskLike, workdir: Path, project_root: Path) -> dict[str, str]:
        return {
            "KAGAN_TASK_ID": task.id,
            "KAGAN_TASK_TITLE": task.title,
            "KAGAN_WORKTREE_PATH": str(workdir),
            "KAGAN_PROJECT_ROOT": str(project_root),
        }

    def _model_for_agent(self, agent_config: AgentConfig) -> str | None:
        key = _AGENT_MODEL_CONFIG_KEY.get(agent_config.short_name.strip().lower())
        if not key:
            return None
        value = getattr(self._config.general, key, None)
        return value if isinstance(value, str) and value.strip() else None

    async def create_session(self, task: TaskLike, worktree_path: Path) -> str:
        """Create session with full context injection."""
        session_name = f"kagan-{task.id}"
        backend = self._resolve_terminal_backend(task)
        session_env = self._build_session_env(task, worktree_path, self._root)

        agent_config = self._get_agent_config(task)
        await self._write_context_files(worktree_path, agent_config, backend)

        startup_prompt = self._build_startup_prompt(task)
        await self._write_startup_bundle(task, worktree_path, session_name, backend, startup_prompt)
        launch_cmd = self._build_launch_command(
            agent_config,
            startup_prompt,
            self._model_for_agent(agent_config),
        )
        if backend == PairTerminalBackend.WEZTERM.value:
            await create_workspace_session(
                session_name,
                worktree_path,
                env=session_env,
                command=launch_cmd,
            )
        elif backend == PairTerminalBackend.TMUX.value:
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

        if backend == PairTerminalBackend.WEZTERM.value:
            await create_workspace_session(
                session_name,
                workdir,
                env=self._build_session_env(task, workdir, self._root),
            )
        else:
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

        from kagan.command_utils import is_windows

        base_cmd = get_os_value(agent_config.interactive_command)
        if not base_cmd:
            return None

        if is_windows():
            try:
                import mslex

                escaped_prompt = mslex.quote(prompt)
            except Exception:
                escaped_prompt = shlex.quote(prompt)
        else:
            escaped_prompt = shlex.quote(prompt)

        match agent_config.short_name:
            case "claude":
                model_flag = f"--model {model} " if model else ""
                return f"{base_cmd} {model_flag}{escaped_prompt}"
            case "opencode":
                model_flag = f"--model {model} " if model else ""
                return f"{base_cmd} {model_flag}--prompt {escaped_prompt}"
            case "kimi":
                model_flag = f"--model {model} " if model else ""
                return f"{base_cmd} {model_flag}--prompt {escaped_prompt}"
            case "copilot":
                # `copilot --prompt` runs one-shot (non-interactive), so keep PAIR mode interactive.
                return base_cmd
            case "codex" | "gemini":
                model_flag = f"--model {model} " if model else ""
                return f"{base_cmd} {model_flag}{escaped_prompt}"
            case _:
                return base_cmd

    async def _attach_tmux_session(self, session_name: str) -> bool:
        """Attach to a tmux session, returning True if attach succeeded."""
        log.debug("Attaching to tmux session: %s", session_name)
        proc = await asyncio.create_subprocess_exec("tmux", "attach-session", "-t", session_name)
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

    async def _attach_wezterm_session(self, session_name: str) -> bool:
        """Open a wezterm window for an existing workspace session."""
        log.debug("Attaching to wezterm workspace: %s", session_name)
        if not await wezterm_workspace_exists(session_name):
            return False
        await run_wezterm("start", "--always-new-process", "--workspace", session_name)
        return True

    async def attach_session(self, task_id: str) -> bool:
        """Attach to session (blocks until detach, then returns to TUI)."""
        session_name = f"kagan-{task_id}"
        backend = await self._resolve_terminal_backend_for_task_id(task_id)
        try:
            if backend == PairTerminalBackend.WEZTERM.value:
                return await self._attach_wezterm_session(session_name)
            if backend == PairTerminalBackend.TMUX.value:
                return await self._attach_tmux_session(session_name)
            worktree_path = await self._workspaces.get_path(task_id)
            if worktree_path is None:
                return False
            return await self._launch_external_launcher(backend, worktree_path)
        except WeztermError:
            return False
        except RuntimeError:
            return False

    async def attach_resolution_session(self, task_id: str) -> bool:
        """Attach to resolution session (blocks until detach)."""
        session_name = self._resolve_session_name(task_id)
        backend = await self._resolve_terminal_backend_for_task_id(task_id)
        try:
            if backend == PairTerminalBackend.WEZTERM.value:
                return await self._attach_wezterm_session(session_name)
            return await self._attach_tmux_session(session_name)
        except WeztermError:
            return False

    async def session_exists(self, task_id: str) -> bool:
        """Check if session exists."""
        session_name = f"kagan-{task_id}"
        backend = await self._resolve_terminal_backend_for_task_id(task_id)
        if backend == PairTerminalBackend.WEZTERM.value:
            with contextlib.suppress(WeztermError):
                return await wezterm_workspace_exists(session_name)
            return False
        if backend in _EXTERNAL_PAIR_BACKENDS:
            worktree_path = await self._workspaces.get_path(task_id)
            if worktree_path is None:
                return False
            return self._bundle_json_path(worktree_path).exists()
        with contextlib.suppress(TmuxError):
            output = await run_tmux("list-sessions", "-F", "#{session_name}")
            return session_name in output.split("\n")
        return False

    async def resolution_session_exists(self, task_id: str) -> bool:
        """Check if resolution session exists."""
        session_name = self._resolve_session_name(task_id)
        backend = await self._resolve_terminal_backend_for_task_id(task_id)
        if backend == PairTerminalBackend.WEZTERM.value:
            with contextlib.suppress(WeztermError):
                return await wezterm_workspace_exists(session_name)
            return False
        with contextlib.suppress(TmuxError):
            output = await run_tmux("list-sessions", "-F", "#{session_name}")
            return session_name in output.split("\n")
        return False

    async def kill_session(self, task_id: str) -> None:
        """Kill session and mark inactive."""
        session_name = f"kagan-{task_id}"
        backend = await self._resolve_terminal_backend_for_task_id(task_id)
        if backend == PairTerminalBackend.WEZTERM.value:
            with contextlib.suppress(WeztermError):
                await kill_workspace(session_name)
        elif backend == PairTerminalBackend.TMUX.value:
            with contextlib.suppress(TmuxError):
                await run_tmux("kill-session", "-t", session_name)
        await self._tasks.close_session_by_external_id(session_name, status=SessionStatus.CLOSED)

    async def kill_resolution_session(self, task_id: str) -> None:
        """Kill resolution session if present."""
        session_name = self._resolve_session_name(task_id)
        backend = await self._resolve_terminal_backend_for_task_id(task_id)
        if backend == PairTerminalBackend.WEZTERM.value:
            with contextlib.suppress(WeztermError):
                await kill_workspace(session_name)
        else:
            with contextlib.suppress(TmuxError):
                await run_tmux("kill-session", "-t", session_name)
        await self._tasks.close_session_by_external_id(
            session_name,
            status=SessionStatus.CLOSED,
        )

    async def _write_context_files(
        self,
        worktree_path: Path,
        agent_config: AgentConfig,
        backend: str,
    ) -> None:
        """Create MCP configuration in worktree (merging if file exists).

        Note: We no longer create CLAUDE.md, AGENTS.md, or CONTEXT.md because:
        - CLAUDE.md/AGENTS.md: Already present in worktree from git clone
        - CONTEXT.md: Redundant with MCP get_context tool
        """
        ignore_entries: list[str] = [".kagan/"]

        mcp_file = await self._write_mcp_config(worktree_path, agent_config)
        if mcp_file:
            ignore_entries.append(mcp_file)

        ignore_entries.extend(await self._write_ide_mcp_configs(worktree_path, backend))

        if ignore_entries:
            modified = await self._ensure_worktree_gitignored(worktree_path, ignore_entries)
            await self._commit_gitignore_if_needed(worktree_path, modified)

    async def _write_ide_mcp_configs(self, worktree_path: Path, backend: str) -> list[str]:
        """Write MCP config files used by redirected IDE launchers."""
        if backend not in _EXTERNAL_PAIR_BACKENDS:
            return []

        server_name = get_mcp_server_name()
        files_written: list[str] = []

        if backend == "vscode":
            vscode_entry = {
                "type": "stdio",
                "command": "kagan",
                "args": ["mcp"],
            }
            await self._merge_json_config(
                worktree_path / ".vscode" / "mcp.json",
                "servers",
                server_name,
                vscode_entry,
            )
            files_written.append(".vscode/mcp.json")

        if backend == "cursor":
            cursor_entry = {
                "command": "kagan",
                "args": ["mcp"],
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

    async def _write_mcp_config(self, worktree_path: Path, agent_config: AgentConfig) -> str | None:
        """Write/merge MCP config based on agent type. Returns filename written or None."""
        import aiofiles

        from kagan.builtin_agents import get_builtin_agent

        builtin = get_builtin_agent(agent_config.short_name)

        if builtin and not builtin.supports_worktree_mcp:
            return None

        server_name = get_mcp_server_name()

        if builtin and builtin.mcp_config_format == "opencode":
            filename = "opencode.json"
            kagan_entry = {
                "type": "local",
                "command": ["kagan", "mcp"],
                "enabled": True,
            }
            mcp_key = "mcp"
        else:
            filename = ".mcp.json"
            kagan_entry = {
                "command": "kagan",
                "args": ["mcp"],
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
        """Auto-commit .gitignore so agents start with clean git status."""
        if not modified:
            return
        proc = await asyncio.create_subprocess_exec(
            "git",
            "-C",
            str(worktree_path),
            "add",
            ".gitignore",
            stdout=asyncio.subprocess.DEVNULL,
            stderr=asyncio.subprocess.DEVNULL,
        )
        await proc.wait()
        proc = await asyncio.create_subprocess_exec(
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

    def _build_startup_prompt(self, task: TaskLike) -> str:
        """Build startup prompt for pair mode."""
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
- When complete: commit your work, then call the `request_review` MCP tool

## MCP Tools Available
{tool_format_note}

**Context Tools:**
- `get_context` - Get full task details (acceptance criteria, scratchpad, linked tasks)
- `get_task` - Look up any task's details by ID (useful for @mentioned tasks)
- `update_scratchpad` - Save progress notes for future reference

**Coordination Tools (USE THESE):**
- `get_parallel_tasks` - Discover concurrent work to avoid merge conflicts
- `get_task(task_id, include_logs=true)` - Get execution logs to learn from prior work

**Completion Tools:**
- `request_review` - Submit work for review (commit first!)

## Coordination Workflow

Before implementing, check for parallel work and historical context:

1. **Check parallel work**: Call `get_parallel_tasks` with your task_id to exclude self.
   Review concurrent tasks to identify overlapping file modifications or shared dependencies.

2. **Learn from history**: Call `get_task(task_id, include_logs=true)` on related completed tasks.
   Avoid repeating failed approaches; reuse successful patterns.

3. **Coordinate strategy**: If overlap exists, plan which files to modify first or wait for.

## Setup Verification

Please confirm you have access to the Kagan MCP tools by:
1. Calling `get_context` with task_id: `{task.id}`
2. Calling `get_parallel_tasks` to check for concurrent work

After confirming MCP access, please:
1. Summarize your understanding of this task (including acceptance criteria from MCP)
2. Report any parallel work that might affect our implementation
3. Ask me if I'm ready to proceed with the implementation

**Wait for my confirmation before beginning any implementation.**
"""
