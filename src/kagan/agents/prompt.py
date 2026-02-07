"""Build run prompts for AUTO mode agents."""

from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.agents.prompt_loader import RUN_PROMPT
from kagan.mcp_naming import get_mcp_server_name

if TYPE_CHECKING:
    from typing import Any

    from kagan.services.types import TaskLike


def build_prompt(
    task: TaskLike,
    run_count: int,
    scratchpad: str,
    hat: Any | None = None,
    user_name: str = "Developer",
    user_email: str = "developer@localhost",
) -> str:
    """Build the prompt for an agent run.

    Args:
        task: The task to build the prompt for.
        run_count: Current run number (1-indexed).
        scratchpad: Previous progress notes from prior runs.
        hat: Optional hat configuration for role-specific instructions.
        user_name: Git user name for Co-authored-by attribution.
        user_email: Git user email for Co-authored-by attribution.

    Returns:
        The formatted prompt string for the agent.
    """

    hat_instructions = ""
    if hat and hasattr(hat, "system_prompt") and hat.system_prompt:
        hat_instructions = hat.system_prompt

    hat_section = f"## Role\n{hat_instructions}" if hat_instructions else ""

    criteria_section = ""
    if task.acceptance_criteria:
        criteria_list = "\n".join(f"- {c}" for c in task.acceptance_criteria)
        criteria_section = f"\n## Acceptance Criteria\n{criteria_list}\n"

    full_description = task.description or "No description provided."
    full_description = full_description + criteria_section

    return RUN_PROMPT.format(
        task_id=task.id,
        run_count=run_count,
        title=task.title,
        description=full_description,
        scratchpad=scratchpad or "(No previous progress - this is run 1)",
        hat_instructions=hat_section,
        user_name=user_name,
        user_email=user_email,
        mcp_server_name=get_mcp_server_name(),
    )
