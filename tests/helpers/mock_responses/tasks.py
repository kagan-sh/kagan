"""Canned ACP responses for task completion and review scenarios.

Provides response builders and pre-built constants that simulate
realistic agent output for task lifecycle testing.
"""

from __future__ import annotations

SIMPLE_IMPLEMENTATION_TEXT = """\
Implemented the requested changes.

<complete/>
"""

SIMPLE_REVIEW_APPROVE_TEXT = """\
Reviewed changes.

<approve summary="Looks good"/>
"""

TASK_COMPLETE_RESPONSE = """\
I've completed the implementation as specified.

## Changes Made

- Created `src/auth/jwt.py` with token generation and validation
- Added `src/auth/middleware.py` for request authentication
- Updated `src/routes/api.py` to use the new middleware
- Added comprehensive tests in `tests/test_auth.py`

All acceptance criteria have been met and tests are passing.

<complete/>
"""

TASK_BLOCKED_RESPONSE = """\
I've encountered an issue that prevents me from completing this task.

The acceptance criteria require database access, but the database \
connection configuration is missing from the environment.

<blocked reason="Missing DATABASE_URL environment variable"/>
"""

REVIEW_APPROVE_RESPONSE = """\
I've reviewed the changes and they look good.

## Review Summary

The implementation correctly addresses the task requirements:
- Code follows project conventions
- Tests cover the main functionality
- No obvious security issues

<approve summary="Implementation is correct and well-tested" \
approach="JWT with refresh tokens, bcrypt password hashing" \
key_files="src/auth/jwt.py, src/auth/middleware.py"/>
"""

REVIEW_APPROVE_SIMPLE_RESPONSE = """\
Changes reviewed and approved.

The implementation is clean and meets all acceptance criteria.

<approve summary="All acceptance criteria met"/>
"""

REVIEW_REJECT_RESPONSE = """\
I've found issues that need to be addressed before approval.

## Issues Found

1. **Missing error handling**: The login function doesn't handle database \
connection errors.
2. **Test coverage**: No tests for the token refresh endpoint.
3. **Security concern**: Password is logged in debug mode.

Please address these issues and request a new review.

<reject reason="Missing error handling, incomplete test coverage, security concern"/>
"""


def make_complete_response(summary: str, files_changed: list[str] | None = None) -> str:
    """Build a task completion response.

    Args:
        summary: Brief description of what was done
        files_changed: Optional list of modified files

    Returns:
        Formatted completion response with <complete/> signal
    """
    parts = [summary]

    if files_changed:
        parts.append("\n## Files Changed\n")
        for f in files_changed:
            parts.append(f"- `{f}`")

    parts.append("\n<complete/>")
    return "\n".join(parts)


def make_blocked_response(reason: str, context: str | None = None) -> str:
    """Build a blocked response.

    Args:
        reason: Short reason for the block (goes in tag attribute)
        context: Optional longer explanation

    Returns:
        Formatted blocked response with <blocked/> signal
    """
    parts = []
    if context:
        parts.append(context)
        parts.append("")

    parts.append(f'<blocked reason="{reason}"/>')
    return "\n".join(parts)


def make_approve_response(
    summary: str,
    approach: str | None = None,
    key_files: str | None = None,
    notes: str | None = None,
) -> str:
    """Build a review approval response.

    Args:
        summary: Brief summary of the approval
        approach: Technical approach used
        key_files: Key files to review
        notes: Optional review notes

    Returns:
        Formatted approval response with <approve/> signal
    """
    parts = []
    if notes:
        parts.append(notes)
        parts.append("")

    attrs = [f'summary="{summary}"']
    if approach:
        attrs.append(f'approach="{approach}"')
    if key_files:
        attrs.append(f'key_files="{key_files}"')

    parts.append(f"<approve {' '.join(attrs)}/>")
    return "\n".join(parts)


def make_reject_response(reason: str, issues: list[str] | None = None) -> str:
    """Build a review rejection response.

    Args:
        reason: Short reason for rejection (goes in tag attribute)
        issues: Optional list of specific issues found

    Returns:
        Formatted rejection response with <reject/> signal
    """
    parts = []
    if issues:
        parts.append("## Issues Found\n")
        for i, issue in enumerate(issues, 1):
            parts.append(f"{i}. {issue}")
        parts.append("")

    parts.append(f'<reject reason="{reason}"/>')
    return "\n".join(parts)


def make_clarification_response(questions: list[str], context: str | None = None) -> str:
    """Build a clarification request response.

    Args:
        questions: List of questions to ask
        context: Optional context before questions

    Returns:
        Formatted clarification request (no signal - expects user input)
    """
    parts = []
    if context:
        parts.append(context)
        parts.append("")

    for i, q in enumerate(questions, 1):
        parts.append(f"{i}. {q}")

    return "\n".join(parts)
