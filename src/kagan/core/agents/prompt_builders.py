"""Agent prompt building, template loading, and conflict resolution instructions."""

from __future__ import annotations

from functools import cache
from importlib.resources import files
from typing import TYPE_CHECKING

from kagan.core.mcp_naming import get_mcp_server_name

if TYPE_CHECKING:
    from typing import Any

    from kagan.core.services.types import TaskLike

# ---------------------------------------------------------------------------
# Prompt template loading
# ---------------------------------------------------------------------------


@cache
def _load_prompt_template(filename: str) -> str:
    """Load a prompt template from package resources."""
    return (files("kagan.core.agents.prompts") / filename).read_text(encoding="utf-8")


RUN_PROMPT = _load_prompt_template("run_prompt.md")
REVIEW_PROMPT = _load_prompt_template("review_prompt.md")


def get_review_prompt(
    title: str,
    task_id: str,
    description: str,
    commits: str,
    diff_summary: str,
) -> str:
    """Get formatted review prompt."""
    return REVIEW_PROMPT.format(
        title=title,
        task_id=task_id,
        description=description,
        commits=commits,
        diff_summary=diff_summary,
    )


# ---------------------------------------------------------------------------
# Run prompt builder
# ---------------------------------------------------------------------------


def build_prompt(
    task: TaskLike,
    run_count: int,
    scratchpad: str,
    hat: Any | None = None,
    user_name: str = "Developer",
    user_email: str = "developer@localhost",
) -> str:
    """Build the prompt for an agent run."""
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
    coordination_guardrails = (
        "## Coordination Guardrails (Overlap-Only, No Inter-Agent Chat)\n"
        "- Use MCP task state as source of truth: "
        "`tasks_list`, `get_task`, and `get_context`.\n"
        "- Do NOT attempt direct/free-form communication with other agents.\n"
        "- If overlap is detected, coordinate by choosing "
        "non-overlapping files or sequencing work.\n"
        "- Record assumptions and overlap decisions in `update_scratchpad`, not chat.\n"
    )

    return RUN_PROMPT.format(
        task_id=task.id,
        run_count=run_count,
        title=task.title,
        description=full_description,
        scratchpad=scratchpad or "(No previous progress - this is run 1)",
        hat_instructions=hat_section,
        coordination_guardrails=coordination_guardrails,
        user_name=user_name,
        user_email=user_email,
        mcp_server_name=get_mcp_server_name(),
    )


# ---------------------------------------------------------------------------
# Conflict resolution instructions
# ---------------------------------------------------------------------------


def build_conflict_resolution_instructions(
    source_branch: str,
    target_branch: str,
    conflict_files: list[str],
    repo_name: str = "",
) -> str:
    """Build agent-ready instructions for resolving rebase conflicts."""
    repo_ctx = f" in {repo_name}" if repo_name else ""
    file_list = "\n".join(f"  - {f}" for f in conflict_files) if conflict_files else "  (unknown)"

    return f"""## Rebase Conflict Resolution Required

A rebase of `{source_branch}` onto `{target_branch}`{repo_ctx} produced conflicts
in {len(conflict_files)} file(s):

{file_list}

### Steps to resolve

1. Run `git rebase {target_branch}` to begin the rebase.
2. For each conflicted file, open it, resolve the conflict markers
   (`<<<<<<<`, `=======`, `>>>>>>>`), and save.
3. Stage resolved files: `git add <file>`.
4. Continue the rebase: `GIT_EDITOR=true git rebase --continue`.
5. Repeat steps 2-4 if additional commits produce conflicts.

### Important

- Preserve the intent of both sides when resolving conflicts.
- Run any relevant tests after resolving to verify correctness.
- Do NOT use `git rebase --skip` unless you are certain the commit is unnecessary.
"""
