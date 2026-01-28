"""Hardcoded prompts for Kagan agents.

All prompts are hardcoded to ensure consistent behavior and avoid
configuration complexity. Customization can be added later based on
actual user needs.
"""

from __future__ import annotations

# =============================================================================
# ITERATION PROMPT (AUTO mode worker agents)
# =============================================================================

ITERATION_PROMPT = """\
# Iteration {iteration} of {max_iterations}

## Task: {title}

{description}

{hat_instructions}

## Your Progress So Far

{scratchpad}

## Instructions

1. Review your previous progress above (if any)
2. Continue working on the task
3. Make incremental progress - don't try to do everything at once
4. Run tests/builds to verify your changes work
5. **CRITICAL: Commit ALL changes before completing**
   - Use semantic commits: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`
   - You CANNOT signal completion with uncommitted changes

## CRITICAL: Response Signal Required

You MUST end your response with exactly ONE of these XML signals:

- `<complete/>` - Task is FULLY DONE and verified working
- `<continue/>` - Made progress, need another iteration to finish
- `<blocked reason="why"/>` - Cannot proceed without human help

**If you completed the task successfully, you MUST output `<complete/>` as the last thing
in your response.**

Example ending:
```
I've implemented the feature and verified it works.
<complete/>
```
"""

# =============================================================================
# REVIEW PROMPT (code review after AUTO completion)
# =============================================================================

REVIEW_PROMPT = """\
# Code Review Request

## Ticket: {title}

**ID:** {ticket_id}
**Description:** {description}

## Changes Made

### Commits
{commits}

### Diff Summary
{diff_summary}

## Review Criteria

Please review the changes against:
1. Does the implementation match the ticket description?
2. Are there any obvious bugs or issues?
3. Is the code reasonably clean and maintainable?

## Your Task

1. Review the changes
2. Provide a brief summary of what was implemented
3. End with exactly ONE signal:

- `<approve summary="Brief 1-2 sentence summary of work done"/>` - Changes are good
- `<reject reason="What needs to be fixed"/>` - Changes need work

Example:
```
The implementation looks good. Added the new feature with proper error handling.
<approve summary="Implemented user authentication with JWT tokens and proper validation"/>
```
"""


def get_review_prompt(
    title: str,
    ticket_id: str,
    description: str,
    commits: str,
    diff_summary: str,
) -> str:
    """Get formatted review prompt.

    Args:
        title: Ticket title.
        ticket_id: Ticket ID.
        description: Ticket description.
        commits: Formatted commit messages.
        diff_summary: Diff statistics summary.

    Returns:
        Formatted review prompt.
    """
    return REVIEW_PROMPT.format(
        title=title,
        ticket_id=ticket_id,
        description=description,
        commits=commits,
        diff_summary=diff_summary,
    )
