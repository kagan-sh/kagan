"""Planner agent support for ticket generation from natural language."""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from kagan.database.models import TicketCreate, TicketPriority

if TYPE_CHECKING:
    from kagan.agents.prompt_loader import PromptLoader

# Pattern to extract ticket blocks from planner response
TICKET_PATTERN = re.compile(r"<ticket>(?P<body>.*?)</ticket>", re.IGNORECASE | re.DOTALL)

# Customizable preamble - users can modify this part
PLANNER_PREAMBLE = """\
You are a project planning assistant. Your job is to take user requests
and create well-structured development tickets.

When the user describes what they want to build or accomplish,
analyze their request and create ONE detailed ticket.

## Guidelines
1. Title should start with a verb (Create, Implement, Fix, Add, Update, etc.)
2. Description should be thorough enough for a developer to understand the task
3. Include acceptance criteria as bullet points
4. If the request is vague, make reasonable assumptions and note them

After outputting the ticket, briefly explain what you created and any assumptions you made.
"""

# Fixed output format - DO NOT let users modify this, parsing depends on it
PLANNER_OUTPUT_FORMAT = """\
## Output Format (Required)

You MUST output your ticket in this exact XML format:

<ticket>
<title>Short, action-oriented title (max 100 chars)</title>
<description>
Detailed description including:
- What needs to be done
- Technical considerations
- Any relevant context
</description>
<acceptance_criteria>
  <criterion>List each acceptance criterion</criterion>
</acceptance_criteria>
<check_command>pytest tests/</check_command>
<priority>medium</priority>
</ticket>

## Priority Levels
- low: Nice to have, no deadline
- medium: Normal priority (default)
- high: Urgent or blocking other work
"""

# Combined prompt for backward compatibility
PLANNER_SYSTEM_PROMPT = PLANNER_PREAMBLE + "\n" + PLANNER_OUTPUT_FORMAT


@dataclass
class ParsedTicket:
    """Result of parsing a ticket from planner output."""

    ticket: TicketCreate
    raw_match: str


def parse_ticket_from_response(response: str) -> ParsedTicket | None:
    """Parse a ticket from the planner agent's response.

    Args:
        response: The full response text from the planner agent.

    Returns:
        ParsedTicket if a valid ticket was found, None otherwise.
    """
    match = TICKET_PATTERN.search(response)
    if not match:
        return None

    body = match.group("body")
    title = _extract_tag(body, "title")
    description = _extract_tag(body, "description")
    if not title or not description:
        return None
    priority_str = (_extract_tag(body, "priority") or "medium").lower()
    acceptance_criteria = _extract_acceptance_criteria(body)
    check_command = _extract_tag(body, "check_command")

    priority_map = {
        "low": TicketPriority.LOW,
        "medium": TicketPriority.MEDIUM,
        "high": TicketPriority.HIGH,
    }
    priority = priority_map.get(priority_str, TicketPriority.MEDIUM)

    ticket = TicketCreate(
        title=title[:200],  # Enforce max length
        description=description,
        priority=priority,
        acceptance_criteria=acceptance_criteria,
        check_command=check_command,
    )

    return ParsedTicket(ticket=ticket, raw_match=match.group(0))


def _extract_tag(body: str, tag: str) -> str | None:
    """Extract a tag's content from the ticket body."""
    match = re.search(rf"<{tag}>(?P<content>.*?)</{tag}>", body, flags=re.IGNORECASE | re.DOTALL)
    if not match:
        return None
    return match.group("content").strip()


def _extract_acceptance_criteria(body: str) -> list[str]:
    """Extract acceptance criteria entries from ticket body."""
    criteria_block = _extract_tag(body, "acceptance_criteria")
    if not criteria_block:
        return []
    entries = re.findall(
        r"<criterion>(?P<content>.*?)</criterion>",
        criteria_block,
        flags=re.IGNORECASE | re.DOTALL,
    )
    return [entry.strip() for entry in entries if entry.strip()]


def build_planner_prompt(
    user_input: str,
    prompt_loader: PromptLoader | None = None,
) -> str:
    """Build the initial prompt for the planner agent.

    Args:
        user_input: The user's natural language request.
        prompt_loader: Optional prompt loader for custom templates.

    Returns:
        Formatted prompt for the planner.
    """
    # Load preamble: prompt_loader > hardcoded default
    # Note: We always append PLANNER_OUTPUT_FORMAT to ensure parsing works
    preamble = prompt_loader.get_planner_prompt() if prompt_loader else PLANNER_PREAMBLE

    return f"""{preamble}

{PLANNER_OUTPUT_FORMAT}

## User Request

{user_input}

Please create a ticket for this request.
"""
