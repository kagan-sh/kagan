"""Tests for MCP tools with mock state manager."""

from __future__ import annotations

from pathlib import Path  # noqa: TC003

import pytest

from kagan.database.models import TicketCreate, TicketStatus
from kagan.mcp.tools import KaganMCPServer

pytestmark = pytest.mark.integration


class TestMCPTools:
    """Tests for MCP tool handlers."""

    async def test_get_context(self, state_manager):
        """Get context returns ticket fields and scratchpad."""
        ticket = await state_manager.create_ticket(
            TicketCreate(
                title="Feature",
                description="Details",
                acceptance_criteria=["Tests pass"],
            )
        )
        await state_manager.update_scratchpad(ticket.id, "Notes")
        server = KaganMCPServer(state_manager)

        context = await server.get_context(ticket.id)

        assert context["ticket_id"] == ticket.id
        assert context["title"] == "Feature"
        assert context["description"] == "Details"
        assert context["acceptance_criteria"] == ["Tests pass"]
        assert context["scratchpad"] == "Notes"

    async def test_update_scratchpad_appends(self, state_manager):
        """update_scratchpad appends to existing content."""
        ticket = await state_manager.create_ticket(TicketCreate(title="Feature"))
        await state_manager.update_scratchpad(ticket.id, "First line")
        server = KaganMCPServer(state_manager)

        result = await server.update_scratchpad(ticket.id, "Second line")

        assert result is True
        scratchpad = await state_manager.get_scratchpad(ticket.id)
        assert scratchpad == "First line\nSecond line"

    async def test_request_review_passes(self, state_manager, monkeypatch):
        """request_review moves ticket to REVIEW on success."""
        ticket = await state_manager.create_ticket(TicketCreate(title="Feature"))
        server = KaganMCPServer(state_manager)

        async def _no_uncommitted(*_args) -> bool:
            return False  # No uncommitted changes

        monkeypatch.setattr(server, "_check_uncommitted_changes", _no_uncommitted)

        result = await server.request_review(ticket.id, "Looks good")

        assert result["status"] == "review"
        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.REVIEW
        assert updated.review_summary == "Looks good"
        assert updated.checks_passed is None

    async def test_request_review_blocks_uncommitted(self, state_manager, monkeypatch):
        """request_review returns error when uncommitted changes exist."""
        ticket = await state_manager.create_ticket(TicketCreate(title="Feature"))
        server = KaganMCPServer(state_manager)

        async def _has_uncommitted(*_args) -> bool:
            return True  # Has uncommitted changes

        monkeypatch.setattr(server, "_check_uncommitted_changes", _has_uncommitted)

        result = await server.request_review(ticket.id, "Looks good")

        assert result["status"] == "error"
        assert "uncommitted" in result["message"].lower()
        # Ticket should not have moved
        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.BACKLOG


class TestCheckUncommittedChanges:
    """Tests for _check_uncommitted_changes filtering logic."""

    async def test_clean_repo_returns_false(self, state_manager, git_repo: Path, monkeypatch):
        """Clean repo should return False (no uncommitted changes)."""
        monkeypatch.chdir(git_repo)
        server = KaganMCPServer(state_manager)

        result = await server._check_uncommitted_changes()

        assert result is False

    async def test_real_changes_returns_true(self, state_manager, git_repo: Path, monkeypatch):
        """Real uncommitted changes should return True."""
        monkeypatch.chdir(git_repo)
        (git_repo / "new_file.py").write_text("# new file")
        server = KaganMCPServer(state_manager)

        result = await server._check_uncommitted_changes()

        assert result is True

    async def test_kagan_dir_ignored(self, state_manager, git_repo: Path, monkeypatch):
        """Untracked .kagan/ directory should be ignored."""
        monkeypatch.chdir(git_repo)
        (git_repo / ".kagan").mkdir()
        (git_repo / ".kagan" / "state.db").write_text("")
        server = KaganMCPServer(state_manager)

        result = await server._check_uncommitted_changes()

        assert result is False

    async def test_opencode_json_ignored(self, state_manager, git_repo: Path, monkeypatch):
        """Untracked opencode.json should be ignored (OpenCode MCP config)."""
        monkeypatch.chdir(git_repo)
        (git_repo / "opencode.json").write_text('{"mcp": {}}')
        server = KaganMCPServer(state_manager)

        result = await server._check_uncommitted_changes()

        assert result is False

    async def test_mcp_json_ignored(self, state_manager, git_repo: Path, monkeypatch):
        """Untracked .mcp.json should be ignored."""
        monkeypatch.chdir(git_repo)
        (git_repo / ".mcp.json").write_text("{}")
        server = KaganMCPServer(state_manager)

        result = await server._check_uncommitted_changes()

        assert result is False

    async def test_claude_md_not_ignored(self, state_manager, git_repo: Path, monkeypatch):
        """Untracked CLAUDE.md should NOT be ignored (we don't generate it anymore)."""
        monkeypatch.chdir(git_repo)
        (git_repo / "CLAUDE.md").write_text("# Claude")
        server = KaganMCPServer(state_manager)

        result = await server._check_uncommitted_changes()

        # CLAUDE.md is no longer a Kagan-generated file, so it should be detected
        assert result is True

    async def test_mixed_kagan_and_real_changes(self, state_manager, git_repo: Path, monkeypatch):
        """Should return True when both Kagan and real changes exist."""
        monkeypatch.chdir(git_repo)
        # Kagan files (should be ignored)
        (git_repo / ".kagan").mkdir()
        (git_repo / ".kagan" / "state.db").write_text("")
        (git_repo / ".mcp.json").write_text("{}")
        # Real file (should not be ignored)
        (git_repo / "feature.py").write_text("# feature")
        server = KaganMCPServer(state_manager)

        result = await server._check_uncommitted_changes()

        assert result is True

    async def test_only_kagan_files_returns_false(self, state_manager, git_repo: Path, monkeypatch):
        """Should return False when only Kagan files are uncommitted."""
        monkeypatch.chdir(git_repo)
        # Create Kagan-generated files (only .kagan/ and MCP configs are generated now)
        (git_repo / ".kagan").mkdir()
        (git_repo / ".kagan" / "config.toml").write_text("")
        (git_repo / ".mcp.json").write_text("{}")
        (git_repo / "opencode.json").write_text("{}")
        server = KaganMCPServer(state_manager)

        result = await server._check_uncommitted_changes()

        assert result is False
