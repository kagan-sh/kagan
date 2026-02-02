"""MCP tool implementations for Kagan."""

from __future__ import annotations

import asyncio
from pathlib import Path

from kagan.constants import KAGAN_GENERATED_PATTERNS
from kagan.database.manager import StateManager  # noqa: TC001
from kagan.database.models import TicketStatus


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
            "scratchpad": scratchpad,
        }

    async def update_scratchpad(self, ticket_id: str, content: str) -> bool:
        """Append to ticket scratchpad."""
        existing = await self._state.get_scratchpad(ticket_id)
        updated = f"{existing}\n{content}".strip() if existing else content
        await self._state.update_scratchpad(ticket_id, updated)
        return True

    async def request_review(self, ticket_id: str, summary: str) -> dict:
        """Mark ticket ready for review.

        For PAIR mode tickets, this moves the ticket to REVIEW status.
        AUTO mode tickets use agent-based review via the scheduler instead.
        """
        ticket = await self._state.get_ticket(ticket_id)
        if ticket is None:
            raise ValueError(f"Ticket not found: {ticket_id}")

        # Check for uncommitted changes before allowing review
        has_uncommitted = await self._check_uncommitted_changes()
        if has_uncommitted:
            return {
                "status": "error",
                "message": "Cannot request review with uncommitted changes. "
                "Please commit your work first.",
            }

        await self._state.update_ticket(
            ticket_id,
            review_summary=summary,
            checks_passed=None,
            status=TicketStatus.REVIEW,
        )
        return {"status": "review", "message": "Ready for merge"}

    async def _check_uncommitted_changes(self) -> bool:
        """Check if there are uncommitted changes in the working directory.

        Excludes Kagan-generated files from the check since they are
        local development metadata, not project files.
        """
        process = await asyncio.create_subprocess_shell(
            "git status --porcelain",
            cwd=Path.cwd(),
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.DEVNULL,
        )
        stdout, _ = await process.communicate()

        if not stdout.strip():
            return False

        # Filter out Kagan-generated files
        for line in stdout.decode().strip().split("\n"):
            if not line:
                continue
            # git status --porcelain format: "XY filename" or "XY  filename -> newname"
            # The filename starts at position 3
            filepath = line[3:].split(" -> ")[0]
            # Check if this file matches any Kagan pattern
            is_kagan_file = any(
                filepath.startswith(p.rstrip("/")) or filepath == p.rstrip("/")
                for p in KAGAN_GENERATED_PATTERNS
            )
            if not is_kagan_file:
                return True  # Found a non-Kagan uncommitted change

        return False  # Only Kagan files are uncommitted
