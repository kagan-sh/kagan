"""Session manager for tmux-backed ticket workflows."""

from __future__ import annotations

import contextlib
import json
import logging
import subprocess
from typing import TYPE_CHECKING

from kagan.agents.config_resolver import resolve_agent_config
from kagan.config import get_os_value
from kagan.sessions.tmux import TmuxError, run_tmux

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.config import AgentConfig, KaganConfig
    from kagan.database.manager import StateManager
    from kagan.database.models import Ticket

log = logging.getLogger(__name__)


class SessionManager:
    """Manages tmux sessions for tickets."""

    def __init__(self, project_root: Path, state: StateManager, config: KaganConfig) -> None:
        self._root = project_root
        self._state = state
        self._config = config

    async def create_session(self, ticket: Ticket, worktree_path: Path) -> str:
        """Create tmux session with full context injection."""
        session_name = f"kagan-{ticket.id}"

        await run_tmux(
            "new-session",
            "-d",
            "-s",
            session_name,
            "-c",
            str(worktree_path),
            "-e",
            f"KAGAN_TICKET_ID={ticket.id}",
            "-e",
            f"KAGAN_TICKET_TITLE={ticket.title}",
            "-e",
            f"KAGAN_WORKTREE_PATH={worktree_path}",
            "-e",
            f"KAGAN_PROJECT_ROOT={self._root}",
        )

        # Get agent config first - needed for context files
        agent_config = self._get_agent_config(ticket)
        await self._write_context_files(worktree_path, agent_config)
        await self._state.mark_session_active(ticket.id, True)

        # Auto-launch the agent's interactive CLI with the startup prompt
        startup_prompt = self._build_startup_prompt(ticket)
        launch_cmd = self._build_launch_command(agent_config, startup_prompt)
        if launch_cmd:
            await run_tmux("send-keys", "-t", session_name, launch_cmd, "Enter")

        return session_name

    def _get_agent_config(self, ticket: Ticket) -> AgentConfig:
        """Get agent config for ticket using unified resolver."""
        return resolve_agent_config(ticket, self._config)

    def _build_launch_command(self, agent_config: AgentConfig, prompt: str) -> str | None:
        """Build CLI launch command with prompt for the agent."""
        import shlex

        base_cmd = get_os_value(agent_config.interactive_command)
        if not base_cmd:
            return None

        escaped_prompt = shlex.quote(prompt)

        # Agent-specific command formats
        if agent_config.short_name == "claude":
            # claude "prompt"
            return f"{base_cmd} {escaped_prompt}"
        elif agent_config.short_name == "opencode":
            # opencode --prompt "prompt"
            return f"{base_cmd} --prompt {escaped_prompt}"
        else:
            # Fallback: just run the base command (no auto-prompt)
            return base_cmd

    def attach_session(self, ticket_id: str) -> bool:
        """Attach to session (blocks until detach, then returns to TUI).

        Returns:
            True if attach was successful (user detached normally).
            False if attach failed (session doesn't exist or tmux error).
        """
        session_name = f"kagan-{ticket_id}"
        log.debug("Attaching to tmux session: %s", session_name)
        result = subprocess.run(["tmux", "attach-session", "-t", session_name])
        if result.returncode != 0:
            log.warning(
                "Failed to attach to session %s (exit code: %d)",
                session_name,
                result.returncode,
            )
            return False
        log.debug("Detached from session: %s", session_name)
        return True

    async def session_exists(self, ticket_id: str) -> bool:
        """Check if session exists."""
        try:
            output = await run_tmux("list-sessions", "-F", "#{session_name}")
            return f"kagan-{ticket_id}" in output.split("\n")
        except TmuxError:
            # No tmux server running = no sessions exist
            return False

    async def kill_session(self, ticket_id: str) -> None:
        """Kill session and mark inactive."""
        with contextlib.suppress(TmuxError):
            await run_tmux("kill-session", "-t", f"kagan-{ticket_id}")
        await self._state.mark_session_active(ticket_id, False)

    async def _write_context_files(self, worktree_path: Path, agent_config: AgentConfig) -> None:
        """Create MCP configuration in worktree (merging if file exists).

        Note: We no longer create CLAUDE.md, AGENTS.md, or CONTEXT.md because:
        - CLAUDE.md/AGENTS.md: Already present in worktree from git clone
        - CONTEXT.md: Redundant with kagan_get_context MCP tool
        """
        mcp_file = self._write_mcp_config(worktree_path, agent_config)
        self._ensure_worktree_gitignored(worktree_path, mcp_file)

    def _write_mcp_config(self, worktree_path: Path, agent_config: AgentConfig) -> str:
        """Write/merge MCP config based on agent type. Returns filename written."""
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
                existing = json.loads(config_path.read_text())
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

        config_path.write_text(json.dumps(config, indent=2))
        return filename

    def _ensure_worktree_gitignored(self, worktree_path: Path, mcp_file: str) -> None:
        """Add Kagan MCP config to worktree's .gitignore."""
        gitignore = worktree_path / ".gitignore"
        # Only the MCP config file needs to be gitignored now
        kagan_entries = [mcp_file]

        existing_content = ""
        if gitignore.exists():
            existing_content = gitignore.read_text()
            existing_lines = set(existing_content.split("\n"))
            # Check if all entries already present
            if all(e in existing_lines for e in kagan_entries):
                return

        # Append Kagan entries
        addition = "\n# Kagan MCP config (auto-generated)\n"
        addition += "\n".join(kagan_entries) + "\n"

        if existing_content and not existing_content.endswith("\n"):
            addition = "\n" + addition

        gitignore.write_text(existing_content + addition)

    def _build_startup_prompt(self, ticket: Ticket) -> str:
        """Build startup prompt for pair mode.

        This includes the task overview plus essential rules that were
        previously in CONTEXT.md. The agent gets full details (acceptance
        criteria and scratchpad) via the kagan_get_context MCP tool.
        """
        desc = ticket.description or "No description provided."
        return f"""Hello! I'm starting a pair programming session for ticket **{ticket.id}**.

Act as a Senior Developer collaborating with me on this implementation.

## Task Overview
**Title:** {ticket.title}

**Description:**
{desc}

## Important Rules
- You are in a git worktree, NOT the main repository
- Only modify files within this worktree
- **COMMIT all changes before requesting review** (use semantic commits: feat:, fix:, docs:, etc.)
- When complete: commit your work, then call `kagan_request_review` MCP tool

## MCP Tools Available
- `kagan_get_context` - Get full ticket details (acceptance criteria, scratchpad)
- `kagan_update_scratchpad` - Save progress notes
- `kagan_request_review` - Submit work for review (commit first!)

## Setup Verification
Please confirm you have access to the Kagan MCP tools by calling the `kagan_get_context` tool.
Use ticket_id: `{ticket.id}`.

After confirming MCP access, please:
1. Summarize your understanding of this task (including acceptance criteria from MCP)
2. Ask me if I'm ready to proceed with the implementation

**Wait for my confirmation before beginning any implementation.**
"""
