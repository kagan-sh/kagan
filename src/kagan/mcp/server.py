"""FastMCP server setup for Kagan."""

from __future__ import annotations

import sys
from pathlib import Path

from mcp.server.fastmcp import FastMCP

from kagan.database.manager import StateManager
from kagan.mcp.tools import KaganMCPServer

mcp = FastMCP("kagan")

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


def main() -> None:
    """Entry point for kagan-mcp command."""
    global _kagan_dir
    _kagan_dir = find_kagan_dir(Path.cwd())
    if not _kagan_dir:
        sys.exit("Error: Not in a Kagan-managed project")
    mcp.run(transport="stdio")
