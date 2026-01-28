"""Tests for SessionManager with mock tmux."""

from __future__ import annotations

from pathlib import Path  # noqa: TC003

import pytest

from kagan.database.models import TicketCreate
from kagan.sessions.manager import SessionManager


@pytest.fixture
def mock_tmux(monkeypatch):
    """Intercept tmux subprocess calls."""
    sessions: dict[str, dict[str, object]] = {}

    async def fake_run_tmux(*args: str) -> str:
        command = args[0]
        if command == "new-session":
            name = args[args.index("-s") + 1]
            cwd = args[args.index("-c") + 1]
            env: dict[str, str] = {}
            for idx, value in enumerate(args):
                if value == "-e" and idx + 1 < len(args):
                    key, _, env_value = args[idx + 1].partition("=")
                    env[key] = env_value
            sessions[name] = {"cwd": cwd, "env": env}
            return ""
        if command == "list-sessions":
            return "\n".join(sorted(sessions.keys()))
        if command == "kill-session":
            name = args[args.index("-t") + 1]
            sessions.pop(name, None)
            return ""
        return ""

    monkeypatch.setattr("kagan.sessions.manager.run_tmux", fake_run_tmux)
    return sessions


class TestSessionManager:
    """Session manager behavior tests."""

    async def test_create_session_writes_context(self, state_manager, mock_tmux, tmp_path: Path):
        project_root = tmp_path / "project"
        worktree_path = tmp_path / "worktree"
        project_root.mkdir()
        worktree_path.mkdir()
        (project_root / "AGENTS.md").write_text("Agents")

        ticket = await state_manager.create_ticket(
            TicketCreate(
                title="Add login",
                description="Implement OAuth",
                acceptance_criteria=["Tests pass"],
                check_command="pytest tests/",
            )
        )
        manager = SessionManager(project_root, state_manager)

        session_name = await manager.create_session(ticket, worktree_path)

        assert session_name in mock_tmux
        env = mock_tmux[session_name]["env"]
        assert env["KAGAN_TICKET_ID"] == ticket.id
        assert env["KAGAN_TICKET_TITLE"] == ticket.title
        assert env["KAGAN_WORKTREE_PATH"] == str(worktree_path)
        assert env["KAGAN_PROJECT_ROOT"] == str(project_root)

        context_path = worktree_path / ".kagan" / "CONTEXT.md"
        assert context_path.exists()
        context = context_path.read_text()
        assert ticket.id in context
        assert "Tests pass" in context

        settings_path = worktree_path / ".claude" / "settings.local.json"
        assert settings_path.exists()

        agents_link = worktree_path / "AGENTS.md"
        assert agents_link.exists()

        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.session_active is True

    async def test_session_exists_and_kill(self, state_manager, mock_tmux, tmp_path: Path):
        project_root = tmp_path / "project"
        worktree_path = tmp_path / "worktree"
        project_root.mkdir()
        worktree_path.mkdir()

        ticket = await state_manager.create_ticket(TicketCreate(title="Work"))
        manager = SessionManager(project_root, state_manager)

        await manager.create_session(ticket, worktree_path)
        assert await manager.session_exists(ticket.id) is True

        await manager.kill_session(ticket.id)
        assert await manager.session_exists(ticket.id) is False

        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.session_active is False
