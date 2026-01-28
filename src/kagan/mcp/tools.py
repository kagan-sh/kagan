"""MCP tool implementations for Kagan."""

from __future__ import annotations

import asyncio
from pathlib import Path

from kagan.database.manager import StateManager  # noqa: TC001
from kagan.database.models import TicketStatus, TicketUpdate

DEFAULT_CHECK_COMMAND = "pytest && ruff check ."


class KaganMCPServer:
    """Handler for MCP tools backed by StateManager."""

    def __init__(self, state_manager: StateManager) -> None:
        self._state = state_manager

    async def get_context(self, ticket_id: str) -> dict:
        """Get ticket context for AI tools."""
        ticket = await self._state.get_ticket(ticket_id)
        if ticket is None:
            raise ValueError(f"Ticket not found: {ticket_id}")
        scratchpad = await self._state.get_scratchpad(ticket_id)
        return {
            "ticket_id": ticket.id,
            "title": ticket.title,
            "description": ticket.description,
            "acceptance_criteria": ticket.acceptance_criteria,
            "check_command": ticket.check_command,
            "scratchpad": scratchpad,
        }

    async def update_scratchpad(self, ticket_id: str, content: str) -> bool:
        """Append to ticket scratchpad."""
        existing = await self._state.get_scratchpad(ticket_id)
        updated = f"{existing}\n{content}".strip() if existing else content
        await self._state.update_scratchpad(ticket_id, updated)
        return True

    async def request_review(self, ticket_id: str, summary: str) -> dict:
        """Mark ticket ready for review. Runs acceptance checks."""
        ticket = await self._state.get_ticket(ticket_id)
        if ticket is None:
            raise ValueError(f"Ticket not found: {ticket_id}")

        checks_passed = await self._run_checks(ticket.check_command)
        update = TicketUpdate(review_summary=summary, checks_passed=checks_passed)
        if checks_passed:
            update.status = TicketStatus.REVIEW
            await self._state.update_ticket(ticket_id, update)
            return {"status": "review", "message": "Ready for merge"}

        await self._state.update_ticket(ticket_id, update)
        return {"status": "failed", "message": "Checks failed"}

    async def _run_checks(self, check_command: str | None) -> bool:
        """Run ticket acceptance checks in the current working directory."""
        command = check_command or DEFAULT_CHECK_COMMAND
        process = await asyncio.create_subprocess_shell(
            command,
            cwd=Path.cwd(),
        )
        return_code = await process.wait()
        return return_code == 0
