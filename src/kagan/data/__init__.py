"""Data module for built-in definitions and static resources."""

from __future__ import annotations

from kagan.data.builtin_agents import (
    BUILTIN_AGENTS,
    get_builtin_agent,
    list_builtin_agents,
)

__all__ = ["BUILTIN_AGENTS", "get_builtin_agent", "list_builtin_agents"]
