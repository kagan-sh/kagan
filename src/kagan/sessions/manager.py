"""Session manager for tmux-backed ticket workflows."""

from __future__ import annotations

import contextlib
import subprocess
from pathlib import Path  # noqa: TC003

from kagan.database.manager import StateManager  # noqa: TC001
from kagan.database.models import Ticket  # noqa: TC001
from kagan.sessions.context import build_context
from kagan.sessions.tmux import TmuxError, run_tmux


class SessionManager:
    """Manages tmux sessions for tickets."""

    def __init__(self, project_root: Path, state: StateManager) -> None:
        self._root = project_root
        self._state = state

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

        await self._write_context_files(ticket, worktree_path)
        await self._state.mark_session_active(ticket.id, True)

        # Auto-launch Claude Code in the session
        await run_tmux("send-keys", "-t", session_name, "claude", "Enter")

        return session_name

    def attach_session(self, ticket_id: str) -> None:
        """Attach to session (blocks until detach, then returns to TUI)."""
        subprocess.run(["tmux", "attach-session", "-t", f"kagan-{ticket_id}"])

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

    async def _write_context_files(self, ticket: Ticket, worktree_path: Path) -> None:
        """Create context and configuration files in worktree."""
        wt_kagan = worktree_path / ".kagan"
        wt_kagan.mkdir(exist_ok=True)
        (wt_kagan / "CONTEXT.md").write_text(build_context(ticket))

        claude_dir = worktree_path / ".claude"
        claude_dir.mkdir(exist_ok=True)
        (claude_dir / "settings.local.json").write_text(
            '{"mcpServers": {"kagan": {"command": "kagan-mcp"}}}'
        )

        agents_md = self._root / "AGENTS.md"
        wt_agents = worktree_path / "AGENTS.md"
        if agents_md.exists() and not wt_agents.exists():
            wt_agents.symlink_to(agents_md)
