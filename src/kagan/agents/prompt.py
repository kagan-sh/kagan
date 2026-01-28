"""Build iteration prompts for AUTO mode agents."""

from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.agents.prompt_loader import ITERATION_PROMPT

if TYPE_CHECKING:
    from typing import Any

    from kagan.database.models import Ticket


def build_prompt(
    ticket: Ticket,
    iteration: int,
    max_iterations: int,
    scratchpad: str,
    hat: Any | None = None,
) -> str:
    """Build the prompt for an agent iteration.

    Args:
        ticket: The ticket to build the prompt for.
        iteration: Current iteration number (1-indexed).
        max_iterations: Maximum allowed iterations.
        scratchpad: Previous progress notes from prior iterations.
        hat: Optional hat configuration for role-specific instructions.

    Returns:
        The formatted prompt string for the agent.
    """
    # Get hat instructions if present
    hat_instructions = ""
    if hat and hasattr(hat, "system_prompt") and hat.system_prompt:
        hat_instructions = hat.system_prompt

    # Format hat instructions with header if present
    hat_section = f"## Role\n{hat_instructions}" if hat_instructions else ""

    # Build acceptance criteria section if present
    criteria_section = ""
    if ticket.acceptance_criteria:
        criteria_list = "\n".join(f"- {c}" for c in ticket.acceptance_criteria)
        criteria_section = f"\n## Acceptance Criteria\n{criteria_list}\n"

    # Combine description with criteria section
    full_description = ticket.description or "No description provided."
    full_description = full_description + criteria_section

    return ITERATION_PROMPT.format(
        iteration=iteration,
        max_iterations=max_iterations,
        title=ticket.title,
        description=full_description,
        scratchpad=scratchpad or "(No previous progress - this is iteration 1)",
        hat_instructions=hat_section,
    )
