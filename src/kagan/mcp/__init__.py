"""MCP server entry point for Kagan."""

from __future__ import annotations

from kagan.mcp.server import main
from kagan.mcp.tools import CoreClientBridge

__all__ = ["CoreClientBridge", "main"]
