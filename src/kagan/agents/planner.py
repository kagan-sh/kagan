"""Planner agent support for ticket generation from natural language."""

from __future__ import annotations

import re
from dataclasses import dataclass

from kagan.database.models import TicketCreate, TicketPriority

# Pattern to extract ticket blocks from planner response
TICKET_PATTERN = re.compile(
    r"<ticket>\s*"
    r"<title>(?P<title>.+?)</title>\s*"
    r"<description>(?P<description>.*?)</description>\s*"
    r"(?:<priority>(?P<priority>low|medium|high)</priority>\s*)?"
    r"</ticket>",
    re.IGNORECASE | re.DOTALL,
)

PLANNER_SYSTEM_PROMPT = """\
You are a project planning assistant. Your job is to take user requests
and create well-structured development tickets.

When the user describes what they want to build or accomplish,
analyze their request and create ONE detailed ticket.

## Output Format

You MUST output your ticket in this exact XML format:

<ticket>
<title>Short, action-oriented title (max 100 chars)</title>
<description>
Detailed description including:
- What needs to be done
- Acceptance criteria
- Technical considerations
- Any relevant context
</description>
<priority>medium</priority>
</ticket>

## Priority Levels
- low: Nice to have, no deadline
- medium: Normal priority (default)
- high: Urgent or blocking other work

## Guidelines
1. Title should start with a verb (Create, Implement, Fix, Add, Update, etc.)
2. Description should be thorough enough for a developer to understand the task
3. Include acceptance criteria as bullet points
4. If the request is vague, make reasonable assumptions and note them

After outputting the ticket, briefly explain what you created and any assumptions you made.
"""


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

    title = match.group("title").strip()
    description = match.group("description").strip()
    priority_str = (match.group("priority") or "medium").lower()

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
    )

    return ParsedTicket(ticket=ticket, raw_match=match.group(0))


def build_planner_prompt(user_input: str) -> str:
    """Build the initial prompt for the planner agent.

    Args:
        user_input: The user's natural language request.

    Returns:
        Formatted prompt for the planner.
    """
    return f"""{PLANNER_SYSTEM_PROMPT}

## User Request

{user_input}

Please create a ticket for this request.
"""
