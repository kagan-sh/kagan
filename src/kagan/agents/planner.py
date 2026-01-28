"""Planner agent support for ticket generation from natural language."""

from __future__ import annotations

import re
from xml.etree import ElementTree as ET

from kagan.database.models import TicketCreate, TicketPriority

# =============================================================================
# PLANNER PROMPT (hardcoded - no customization)
# =============================================================================

PLANNER_PROMPT = """\
You are a planning assistant that creates development tickets in XML format.

## Guidelines
1. Title should start with a verb (Create, Implement, Fix, Add, Update, etc.)
2. Description should be thorough enough for a developer to understand the task
3. Include 2-5 acceptance criteria as bullet points
4. If the request is vague, ask 1-2 clarifying questions first

## CRITICAL: Output Format
When creating tickets, you MUST output them in this EXACT XML format:

<plan>
<ticket>
  <title>Verb + clear objective</title>
  <type>AUTO or PAIR</type>
  <description>What to build and why</description>
  <acceptance_criteria>
    <criterion>Criterion 1</criterion>
    <criterion>Criterion 2</criterion>
  </acceptance_criteria>
  <priority>medium</priority>
</ticket>
</plan>

## Ticket Types - Assign Based on Task Nature

**AUTO** - AI completes autonomously:
- Bug fixes with clear steps
- Adding logging/metrics
- Writing tests
- Code refactoring
- Input validation
- Dependency updates

**PAIR** - Human collaboration needed:
- New feature design
- UX/UI decisions
- API design
- Architecture choices
- Security changes

## Your Workflow
1. If request is clear, output <plan> immediately with tickets
2. If unclear, ask 1-2 questions first, then output <plan>
3. Break requests into 2-5 tickets
4. Assign AUTO or PAIR based on task nature

## Priority: low | medium | high

IMPORTANT: Always output the actual <plan> XML block with tickets.
Never just describe what tickets you would create.
"""


def parse_plan(response: str) -> list[TicketCreate]:
    """Parse multiple tickets from agent response using stdlib XML parser.

    Returns empty list if no <plan> block found or parsing fails.
    """

    match = re.search(r"<plan>(.*?)</plan>", response, re.DOTALL | re.IGNORECASE)
    if not match:
        return []

    try:
        root = ET.fromstring(f"<root>{match.group(1)}</root>")
    except ET.ParseError:
        return []

    return [_element_to_ticket(el) for el in root.findall("ticket")]


def _element_to_ticket(el: ET.Element) -> TicketCreate:
    """Convert XML element to TicketCreate. Pure function."""
    from kagan.database.models import TicketType

    def text(tag: str, default: str = "") -> str:
        child = el.find(tag)
        return (child.text or "").strip() if child is not None else default

    def criteria() -> list[str]:
        ac = el.find("acceptance_criteria")
        if ac is None:
            return []
        return [c.text.strip() for c in ac.findall("criterion") if c.text]

    type_str = text("type", "PAIR").upper()
    ticket_type = TicketType.AUTO if type_str == "AUTO" else TicketType.PAIR

    priority_map = {"low": TicketPriority.LOW, "high": TicketPriority.HIGH}
    priority = priority_map.get(text("priority", "medium").lower(), TicketPriority.MEDIUM)

    return TicketCreate(
        title=text("title", "Untitled")[:200],
        description=text("description"),
        ticket_type=ticket_type,
        priority=priority,
        acceptance_criteria=criteria(),
    )


def build_planner_prompt(user_input: str) -> str:
    """Build the prompt for the planner agent.

    Args:
        user_input: The user's natural language request.

    Returns:
        Formatted prompt for the planner.
    """
    return f"""{PLANNER_PROMPT}

## User Request

{user_input}

Output the <plan> XML block with tickets now.
"""
