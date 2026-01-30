"""Planner agent support for ticket generation from natural language."""

from __future__ import annotations

import re
from typing import TYPE_CHECKING
from xml.etree import ElementTree as ET

from kagan.database.models import TicketCreate, TicketPriority

if TYPE_CHECKING:
    from kagan.acp import protocol

# =============================================================================
# PLANNER PROMPT (hardcoded - no customization)
# =============================================================================

PLANNER_PROMPT = """\
You are a PLANNING ASSISTANT that creates development tickets in XML format.

## CRITICAL CONSTRAINTS - READ CAREFULLY
1. You are a PLANNER, NOT an implementer or worker
2. You MUST NOT write files, create content, or execute code
3. You MUST NOT use terminal commands or filesystem write operations
4. Your ONLY outputs should be:
   - Clarifying questions (if the request is unclear)
   - <todos> blocks showing your planning steps
   - <plan> XML blocks containing tickets for workers to execute

## What You Should Do
- Analyze user requests and break them into actionable tickets
- Ask 1-2 clarifying questions if the request is vague
- Create tickets that DESCRIBE what should be built, not build it yourself
- Assign appropriate type (AUTO/PAIR) based on complexity

## What You Should NOT Do
- Create files (scripts, documents, code, markdown, etc.)
- Execute shell commands
- Write or modify any content directly
- "Help" by taking action beyond planning

If the user asks to "create a script" or "write a document":
- Create a TICKET describing what a worker agent should build
- Do NOT create the file yourself

## Guidelines
1. Title should start with a verb (Create, Implement, Fix, Add, Update, etc.)
2. Description should be thorough enough for a developer to understand the task
3. Include 2-5 acceptance criteria as bullet points
4. If the request is vague, ask 1-2 clarifying questions first

## CRITICAL: Output Format
When creating tickets, you MUST output them in this EXACT XML format:

<todos>
  <todo status="pending">First step you'll take</todo>
  <todo status="pending">Second step</todo>
  <todo status="pending">Third step</todo>
</todos>

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

## Todos Block
Output a <todos> block BEFORE the <plan> to show your working steps:
- status="pending" for steps not yet started
- status="in_progress" for the current step
- status="completed" for finished steps
- Keep todos concise (1 line each, max 5-7 todos)

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
1. Output <todos> showing your planned steps
2. If request is clear, output <plan> immediately with tickets
3. If unclear, ask 1-2 questions first, then output <plan>
4. Break requests into 2-5 tickets
5. Assign AUTO or PAIR based on task nature

## Priority: low | medium | high

IMPORTANT: Always output the actual <plan> XML block with tickets.
Never just describe what tickets you would create.
Always include a <todos> block to show your working steps.
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


def parse_todos(response: str) -> list[protocol.PlanEntry]:
    """Parse todos from agent response for PlanDisplay widget.

    Extracts <todos> block and converts to PlanEntry list.
    Returns empty list if no <todos> block found or parsing fails.
    """
    match = re.search(r"<todos>(.*?)</todos>", response, re.DOTALL | re.IGNORECASE)
    if not match:
        return []

    try:
        root = ET.fromstring(f"<root>{match.group(1)}</root>")
    except ET.ParseError:
        return []

    entries: list[protocol.PlanEntry] = []
    for el in root.findall("todo"):
        content = (el.text or "").strip()
        if not content:
            continue

        status = el.get("status", "pending")
        # Normalize status to valid PlanEntry values
        if status not in ("pending", "in_progress", "completed", "failed"):
            status = "pending"

        entries.append({"content": content, "status": status})

    return entries


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
