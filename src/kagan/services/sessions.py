"""Session manager for tmux-backed task workflows."""

from __future__ import annotations

import asyncio
import contextlib
import json
import logging
from typing import TYPE_CHECKING, Protocol

from kagan.config import get_os_value
from kagan.sessions.tmux import TmuxError, run_tmux

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.config import AgentConfig, KaganConfig
    from kagan.services.tasks import TaskService
    from kagan.services.types import TaskLike

log = logging.getLogger(__name__)


class SessionService(Protocol):
    """Service interface for session operations."""

    async def create_session(self, task: TaskLike, worktree_path: Path) -> str: ...

    async def create_resolution_session(self, task: TaskLike, workdir: Path) -> str: ...

    async def attach_session(self, task_id: str) -> bool: ...

    async def attach_resolution_session(self, task_id: str) -> bool: ...

    async def session_exists(self, task_id: str) -> bool: ...

    async def resolution_session_exists(self, task_id: str) -> bool: ...

    async def kill_session(self, task_id: str) -> None: ...

    async def kill_resolution_session(self, task_id: str) -> None: ...


class SessionServiceImpl:
    """Manages tmux sessions for tasks."""

    def __init__(self, project_root: Path, task_service: TaskService, config: KaganConfig) -> None:
        self._root = project_root
        self._tasks = task_service
        self._config = config

    async def create_session(self, task: TaskLike, worktree_path: Path) -> str:
        """Create tmux session with full context injection."""
        session_name = f"kagan-{task.id}"

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

        # Get agent config first - needed for context files
        agent_config = self._get_agent_config(task)
        await self._write_context_files(worktree_path, agent_config)
        await self._tasks.mark_session_active(task.id, True)

        # Resolve model from config
        model = None
        if "claude" in agent_config.identity.lower():
            model = self._config.general.default_model_claude
        elif "opencode" in agent_config.identity.lower():
            model = self._config.general.default_model_opencode

        # Auto-launch the agent's interactive CLI with the startup prompt
        startup_prompt = self._build_startup_prompt(task)
        launch_cmd = self._build_launch_command(agent_config, startup_prompt, model)
        if launch_cmd:
            await run_tmux("send-keys", "-t", session_name, launch_cmd, "Enter")

        return session_name

    def _resolve_session_name(self, task_id: str) -> str:
        return f"kagan-resolve-{task_id}"

    async def create_resolution_session(self, task: TaskLike, workdir: Path) -> str:
        """Create tmux session for manual conflict resolution."""
        session_name = self._resolve_session_name(task.id)

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

        # Seed basic context for conflict resolution.
        await run_tmux("send-keys", "-t", session_name, "git status", "Enter")
        await run_tmux("send-keys", "-t", session_name, "git diff", "Enter")

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

        base_cmd = get_os_value(agent_config.interactive_command)
        if not base_cmd:
            return None

        escaped_prompt = shlex.quote(prompt)

        # Agent-specific command formats with optional model flag
        if agent_config.short_name == "claude":
            # claude --model opus "prompt"
            model_flag = f"--model {model} " if model else ""
            return f"{base_cmd} {model_flag}{escaped_prompt}"
        elif agent_config.short_name == "opencode":
            # opencode --model anthropic/claude-sonnet-4-5 --prompt "prompt"
            model_flag = f"--model {model} " if model else ""
            return f"{base_cmd} {model_flag}--prompt {escaped_prompt}"
        else:
            # Fallback: just run the base command (no auto-prompt)
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

    async def attach_session(self, task_id: str) -> bool:
        """Attach to session (blocks until detach, then returns to TUI)."""
        session_name = f"kagan-{task_id}"
        return await self._attach_tmux_session(session_name)

    async def attach_resolution_session(self, task_id: str) -> bool:
        """Attach to resolution session (blocks until detach)."""
        session_name = self._resolve_session_name(task_id)
        return await self._attach_tmux_session(session_name)

    async def session_exists(self, task_id: str) -> bool:
        """Check if session exists."""
        try:
            output = await run_tmux("list-sessions", "-F", "#{session_name}")
            return f"kagan-{task_id}" in output.split("\n")
        except TmuxError:
            # No tmux server running = no sessions exist
            return False

    async def resolution_session_exists(self, task_id: str) -> bool:
        """Check if resolution session exists."""
        try:
            output = await run_tmux("list-sessions", "-F", "#{session_name}")
            return self._resolve_session_name(task_id) in output.split("\n")
        except TmuxError:
            return False

    async def kill_session(self, task_id: str) -> None:
        """Kill session and mark inactive."""
        with contextlib.suppress(TmuxError):
            await run_tmux("kill-session", "-t", f"kagan-{task_id}")
        await self._tasks.mark_session_active(task_id, False)

    async def kill_resolution_session(self, task_id: str) -> None:
        """Kill resolution session if present."""
        with contextlib.suppress(TmuxError):
            await run_tmux("kill-session", "-t", self._resolve_session_name(task_id))

    async def _write_context_files(self, worktree_path: Path, agent_config: AgentConfig) -> None:
        """Create MCP configuration in worktree (merging if file exists).

        Note: We no longer create CLAUDE.md, AGENTS.md, or CONTEXT.md because:
        - CLAUDE.md/AGENTS.md: Already present in worktree from git clone
        - CONTEXT.md: Redundant with kagan_get_context MCP tool
        """
        mcp_file = await self._write_mcp_config(worktree_path, agent_config)
        await self._ensure_worktree_gitignored(worktree_path, mcp_file)

    async def _write_mcp_config(self, worktree_path: Path, agent_config: AgentConfig) -> str:
        """Write/merge MCP config based on agent type. Returns filename written."""
        import aiofiles

        from kagan.data.builtin_agents import get_builtin_agent

        builtin = get_builtin_agent(agent_config.short_name)

        if builtin and builtin.mcp_config_format == "opencode":
            # OpenCode format: opencode.json with {"mcp": {"name": {...}}}
            filename = "opencode.json"
            kagan_entry = {
                "type": "local",
                "command": ["kagan", "mcp"],
                "enabled": True,
            }
            mcp_key = "mcp"
        else:
            # Default: Claude Code format - .mcp.json with {"mcpServers": {"name": {...}}}
            filename = ".mcp.json"
            kagan_entry = {
                "command": "kagan",
                "args": ["mcp"],
            }
            mcp_key = "mcpServers"

        config_path = worktree_path / filename

        # Merge with existing config if present
        if config_path.exists():
            try:
                async with aiofiles.open(config_path, encoding="utf-8") as f:
                    content = await f.read()
                existing = json.loads(content)
            except json.JSONDecodeError:
                existing = {}
            if mcp_key not in existing:
                existing[mcp_key] = {}
            existing[mcp_key]["kagan"] = kagan_entry
            config = existing
        else:
            config: dict[str, object] = {mcp_key: {"kagan": kagan_entry}}
            if filename == "opencode.json":
                config["$schema"] = "https://opencode.ai/config.json"

        async with aiofiles.open(config_path, "w", encoding="utf-8") as f:
            await f.write(json.dumps(config, indent=2))
        return filename

    async def _ensure_worktree_gitignored(self, worktree_path: Path, mcp_file: str) -> None:
        """Add Kagan MCP config to worktree's .gitignore."""
        import aiofiles

        gitignore = worktree_path / ".gitignore"
        # Only the MCP config file needs to be gitignored now
        kagan_entries = [mcp_file]

        existing_content = ""
        if gitignore.exists():
            async with aiofiles.open(gitignore, encoding="utf-8") as f:
                existing_content = await f.read()
            existing_lines = set(existing_content.split("\n"))
            # Check if all entries already present
            if all(e in existing_lines for e in kagan_entries):
                return

        # Append Kagan entries
        addition = "\n# Kagan MCP config (auto-generated)\n"
        addition += "\n".join(kagan_entries) + "\n"

        if existing_content and not existing_content.endswith("\n"):
            addition = "\n" + addition

        async with aiofiles.open(gitignore, "w", encoding="utf-8") as f:
            await f.write(existing_content + addition)

    def _build_startup_prompt(self, task: TaskLike) -> str:
        """Build startup prompt for pair mode.

        This includes the task overview plus essential rules that were
        previously in CONTEXT.md. The agent gets full details (acceptance
        criteria and scratchpad) via the kagan_get_context MCP tool.
        """
        desc = task.description or "No description provided."
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
- When complete: commit your work, then call `kagan_request_review` MCP tool

## MCP Tools Available

**Context Tools:**
- `kagan_get_context` - Get full task details (acceptance criteria, scratchpad)
- `kagan_update_scratchpad` - Save progress notes for future reference

**Coordination Tools (USE THESE):**
- `kagan_get_parallel_tasks` - Discover concurrent work to avoid merge conflicts
- `kagan_get_agent_logs` - Get execution logs from any task to learn from prior work

**Completion Tools:**
- `kagan_request_review` - Submit work for review (commit first!)

## Coordination Workflow

Before implementing, check for parallel work and historical context:

1. **Check parallel work**: Call `kagan_get_parallel_tasks` with your task_id to exclude self.
   Review concurrent tasks to identify overlapping file modifications or shared dependencies.

2. **Learn from history**: Call `kagan_get_agent_logs` on related completed tasks.
   Avoid repeating failed approaches; reuse successful patterns.

3. **Coordinate strategy**: If overlap exists, plan which files to modify first or wait for.

## Setup Verification

Please confirm you have access to the Kagan MCP tools by:
1. Calling `kagan_get_context` with task_id: `{task.id}`
2. Calling `kagan_get_parallel_tasks` to check for concurrent work

After confirming MCP access, please:
1. Summarize your understanding of this task (including acceptance criteria from MCP)
2. Report any parallel work that might affect our implementation
3. Ask me if I'm ready to proceed with the implementation

**Wait for my confirmation before beginning any implementation.**
"""
