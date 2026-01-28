"""Build iteration prompts for AUTO mode agents."""

from __future__ import annotations

from functools import cache
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Any

    from kagan.agents.prompt_loader import PromptLoader
    from kagan.database.models import Ticket

TEMPLATE_PATH = Path(__file__).parent.parent / "prompts" / "iteration.md"


@cache
def _load_template() -> str:
    """Load iteration prompt template from file or use fallback."""
    if TEMPLATE_PATH.exists():
        return TEMPLATE_PATH.read_text()
    # Fallback inline template
    return """# Iteration {iteration} of {max_iterations}

## Task: {title}

{description}

{hat_instructions}

## Your Progress So Far
{scratchpad}

## CRITICAL: Response Signal Required

You MUST end your response with exactly ONE of these XML signals:
- `<complete/>` - Task is FULLY DONE and verified working
- `<continue/>` - Made progress, need another iteration
- `<blocked reason="why"/>` - Cannot proceed without human help

**If you completed the task, output `<complete/>` as the last thing in your response.**
"""


def build_prompt(
    ticket: Ticket,
    iteration: int,
    max_iterations: int,
    scratchpad: str,
    hat: Any | None = None,
    prompt_loader: PromptLoader | None = None,
) -> str:
    """Build the prompt for an agent iteration.

    Args:
        ticket: The ticket to build the prompt for.
        iteration: Current iteration number (1-indexed).
        max_iterations: Maximum allowed iterations.
        scratchpad: Previous progress notes from prior iterations.
        hat: Optional hat configuration for role-specific instructions.
        prompt_loader: Optional prompt loader for custom templates.

    Returns:
        The formatted prompt string for the agent.
    """
    # Load template: prompt_loader > builtin
    if prompt_loader:
        template = prompt_loader.get_worker_prompt()
        hat_instructions = prompt_loader.get_hat_instructions(hat)
    else:
        template = _load_template()
        hat_instructions = hat.system_prompt if hat and hat.system_prompt else ""

    # Format hat instructions with header if present
    hat_section = f"## Role\n{hat_instructions}" if hat_instructions else ""

    # Build acceptance criteria section if present
    criteria_section = ""
    if ticket.acceptance_criteria:
        criteria_list = "\n".join(f"- {c}" for c in ticket.acceptance_criteria)
        criteria_section = f"\n## Acceptance Criteria\n{criteria_list}\n"

    # Build check command section if present
    check_section = ""
    if ticket.check_command:
        check_section = (
            f"\n## Verification Command\nRun this to verify your work:\n"
            f"```\n{ticket.check_command}\n```\n"
        )

    # Combine description with criteria and check sections
    full_description = ticket.description or "No description provided."
    full_description = full_description + criteria_section + check_section

    return template.format(
        iteration=iteration,
        max_iterations=max_iterations,
        title=ticket.title,
        description=full_description,
        scratchpad=scratchpad or "(No previous progress - this is iteration 1)",
        hat_instructions=hat_section,
    )
