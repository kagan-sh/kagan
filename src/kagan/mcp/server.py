"""FastMCP server setup for Kagan."""

from __future__ import annotations

import sys
from pathlib import Path

from mcp.server.fastmcp import FastMCP

from kagan.agents import planner as planner_models
from kagan.database.manager import StateManager
from kagan.mcp.tools import KaganMCPServer

_state_manager: StateManager | None = None
_server: KaganMCPServer | None = None
_kagan_dir: Path | None = None


def find_kagan_dir(start: Path) -> Path | None:
    """Find .kagan directory by traversing up."""
    current = start.resolve()
    while current != current.parent:
        if (current / ".kagan").is_dir():
            return current / ".kagan"
        current = current.parent
    return None


async def _get_state_manager() -> StateManager:
    """Get or create the global StateManager."""
    global _state_manager
    if _state_manager is None:
        kagan_dir = _kagan_dir or find_kagan_dir(Path.cwd())
        if kagan_dir is None:
            raise RuntimeError("Not in a Kagan-managed project (.kagan not found)")
        _state_manager = StateManager(kagan_dir / "state.db")
        await _state_manager.initialize()
    return _state_manager


async def _get_server() -> KaganMCPServer:
    """Get or create the MCP server wrapper."""
    global _server
    if _server is None:
        _server = KaganMCPServer(await _get_state_manager())
    return _server


def _create_mcp_server(readonly: bool = False) -> FastMCP:
    """Create FastMCP instance with conditional tool registration."""
    mcp = FastMCP("kagan")

    # Read-only tools (always registered)
    @mcp.tool()
    async def propose_plan(
        tickets: list[planner_models.ProposedTicket],
        todos: list[planner_models.ProposedTodo] | None = None,
    ) -> dict:
        """Submit a structured plan proposal for planner mode."""
        proposal = planner_models.PlanProposal.model_validate(
            {"tickets": tickets, "todos": todos or []}
        )
        return {
            "status": "received",
            "ticket_count": len(proposal.tickets),
            "todo_count": len(proposal.todos),
        }

    @mcp.tool()
    async def get_parallel_tickets(exclude_ticket_id: str | None = None) -> list[dict]:
        """Get all IN_PROGRESS tickets for coordination.

        Use to discover concurrent work and minimize merge conflicts.
        Pass your own ticket_id to exclude it from results.
        """
        return await (await _get_server()).get_parallel_tickets(exclude_ticket_id)

    @mcp.tool()
    async def get_agent_logs(
        ticket_id: str, log_type: str = "implementation", limit: int = 1
    ) -> list[dict]:
        """Get agent execution logs from any ticket.

        Use to learn from prior executions and avoid repeating failed approaches.
        Returns most recent N iterations (default 1).
        """
        return await (await _get_server()).get_agent_logs(ticket_id, log_type, limit)

    # Full-mode tools (PAIR mode only)
    if not readonly:

        @mcp.tool()
        async def get_context(ticket_id: str) -> dict:
            """Get ticket context for AI tools."""
            return await (await _get_server()).get_context(ticket_id)

        @mcp.tool()
        async def update_scratchpad(ticket_id: str, content: str) -> bool:
            """Append to ticket scratchpad."""
            return await (await _get_server()).update_scratchpad(ticket_id, content)

        @mcp.tool()
        async def request_review(ticket_id: str, summary: str) -> dict:
            """Mark ticket ready for review. Runs acceptance checks."""
            return await (await _get_server()).request_review(ticket_id, summary)

    return mcp


def main(readonly: bool = False) -> None:
    """Entry point for kagan-mcp command."""
    global _kagan_dir, _state_manager, _server
    _state_manager = None  # Reset for fresh instance
    _server = None
    _kagan_dir = find_kagan_dir(Path.cwd())
    if not _kagan_dir:
        sys.exit("Error: Not in a Kagan-managed project")
    mcp = _create_mcp_server(readonly=readonly)
    mcp.run(transport="stdio")
