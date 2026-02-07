"""Utilities for interpreting agent process termination messages."""

from __future__ import annotations

import re

_EXIT_CODE_PATTERN = re.compile(r"Agent exited with code (?P<code>-?\d+)")
SIGTERM_EXIT_CODE = -15


def parse_agent_exit_code(message: str) -> int | None:
    """Extract process exit code from an agent failure message."""
    match = _EXIT_CODE_PATTERN.search(message)
    if match is None:
        return None
    try:
        return int(match.group("code"))
    except ValueError:
        return None


def is_graceful_agent_termination(message: str) -> bool:
    """Return True when failure message represents expected SIGTERM cancellation."""
    return parse_agent_exit_code(message) == SIGTERM_EXIT_CODE
