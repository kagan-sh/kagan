"""Context file generation for ticket sessions."""

from __future__ import annotations

from kagan.database.models import Ticket  # noqa: TC001

DEFAULT_CHECK_COMMAND = "pytest && ruff check ."


def build_context(ticket: Ticket) -> str:
    """Generate CONTEXT.md content for a ticket session."""
    criteria_lines = "\n".join(f"- {item}" for item in ticket.acceptance_criteria)
    if not criteria_lines:
        criteria_lines = "- No specific criteria"
    description = ticket.description or "No description provided."
    check_command = ticket.check_command or DEFAULT_CHECK_COMMAND
    return f"""# Ticket: {ticket.id} - {ticket.title}

## Description
{description}

## Acceptance Criteria
{criteria_lines}

## Rules
- You are in a git worktree, NOT the main repository
- Only modify files within this worktree
- Use `kagan_get_context` MCP tool to refresh ticket info
- Use `kagan_update_scratchpad` to save progress notes
- When complete: call `kagan_request_review` MCP tool

## Check Command
{check_command}
"""
