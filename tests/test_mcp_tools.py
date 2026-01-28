"""Tests for MCP tools with mock state manager."""

from __future__ import annotations

from kagan.database.models import TicketCreate, TicketStatus
from kagan.mcp.tools import KaganMCPServer


class TestMCPTools:
    """Tests for MCP tool handlers."""

    async def test_get_context(self, state_manager):
        """Get context returns ticket fields and scratchpad."""
        ticket = await state_manager.create_ticket(
            TicketCreate(
                title="Feature",
                description="Details",
                acceptance_criteria=["Tests pass"],
                check_command="true",
            )
        )
        await state_manager.update_scratchpad(ticket.id, "Notes")
        server = KaganMCPServer(state_manager)

        context = await server.get_context(ticket.id)

        assert context["ticket_id"] == ticket.id
        assert context["title"] == "Feature"
        assert context["description"] == "Details"
        assert context["acceptance_criteria"] == ["Tests pass"]
        assert context["check_command"] == "true"
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

        async def _checks(*_args) -> bool:
            return True

        monkeypatch.setattr(server, "_run_checks", _checks)

        result = await server.request_review(ticket.id, "Looks good")

        assert result["status"] == "review"
        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.REVIEW
        assert updated.review_summary == "Looks good"
        assert updated.checks_passed is True

    async def test_request_review_fails(self, state_manager, monkeypatch):
        """request_review leaves status unchanged on failure."""
        ticket = await state_manager.create_ticket(TicketCreate(title="Feature"))
        server = KaganMCPServer(state_manager)

        async def _checks(*_args) -> bool:
            return False

        monkeypatch.setattr(server, "_run_checks", _checks)

        result = await server.request_review(ticket.id, "Needs work")

        assert result["status"] == "failed"
        updated = await state_manager.get_ticket(ticket.id)
        assert updated is not None
        assert updated.status == TicketStatus.BACKLOG
        assert updated.review_summary == "Needs work"
        assert updated.checks_passed is False
