"""Canned ACP responses for snapshot testing.

These mock responses simulate realistic agent output for different scenarios,
triggering the expected UI states without running actual AI.
"""

from __future__ import annotations

from typing import Any


def make_propose_plan_tool_call(
    tool_call_id: str = "tc-plan-001",
    tasks: list[dict[str, Any]] | None = None,
    todos: list[dict[str, str]] | None = None,
) -> dict[str, Any]:
    """Create a propose_plan tool call structure.

    Args:
        tool_call_id: Unique identifier for the tool call
        tasks: List of task definitions
        todos: List of todo items for the plan

    Returns:
        Tool call dict that can be set via MockAgent.set_tool_calls()
    """
    if tasks is None:
        tasks = [
            {
                "title": "Implement user authentication",
                "type": "AUTO",
                "description": "Add JWT-based authentication to the API endpoints",
                "acceptance_criteria": [
                    "Login endpoint returns JWT token",
                    "Protected endpoints require valid token",
                    "Token expiry is handled correctly",
                ],
                "priority": "high",
            }
        ]

    if todos is None:
        todos = [
            {"content": "Analyze authentication requirements", "status": "completed"},
            {"content": "Design task structure", "status": "completed"},
            {"content": "Create implementation plan", "status": "in_progress"},
        ]

    return {
        tool_call_id: {
            "sessionUpdate": "tool_call",
            "toolCallId": tool_call_id,
            "name": "propose_plan",
            "title": "propose_plan",
            "status": "completed",
            "arguments": {"tasks": tasks, "todos": todos},
        }
    }


PLAN_PROPOSAL_RESPONSE = """\
I've analyzed your request and created a development plan.

Let me propose a structured approach to implement this feature.
"""

PLAN_PROPOSAL_TOOL_CALLS = make_propose_plan_tool_call(
    tool_call_id="tc-plan-001",
    tasks=[
        {
            "title": "Implement user authentication",
            "type": "AUTO",
            "description": (
                "Add JWT-based authentication to the API endpoints. "
                "This includes login, token refresh, and logout functionality."
            ),
            "acceptance_criteria": [
                "Login endpoint accepts email/password and returns JWT",
                "Protected endpoints return 401 without valid token",
                "Token refresh endpoint extends session",
                "Logout invalidates the current token",
            ],
            "priority": "high",
        },
        {
            "title": "Add user registration flow",
            "type": "AUTO",
            "description": (
                "Create user registration with email verification. "
                "New users receive a verification email before account activation."
            ),
            "acceptance_criteria": [
                "Registration endpoint creates inactive user",
                "Verification email is sent with unique token",
                "Verification link activates user account",
            ],
            "priority": "medium",
        },
    ],
    todos=[
        {"content": "Analyze authentication requirements", "status": "completed"},
        {"content": "Design JWT token structure", "status": "completed"},
        {"content": "Create task breakdown", "status": "completed"},
        {"content": "Validate against security best practices", "status": "in_progress"},
    ],
)


MULTI_TASK_PLAN_TOOL_CALLS = make_propose_plan_tool_call(
    tool_call_id="tc-multi-001",
    tasks=[
        {
            "title": "Create database schema for users",
            "type": "AUTO",
            "description": "Design and implement the users table with proper indexes.",
            "acceptance_criteria": [
                "Users table has id, email, password_hash columns",
                "Email has unique constraint",
                "Created migration is reversible",
            ],
            "priority": "high",
        },
        {
            "title": "Implement password hashing utility",
            "type": "AUTO",
            "description": "Create utility functions for secure password hashing using bcrypt.",
            "acceptance_criteria": [
                "Hash function uses bcrypt with cost factor 12",
                "Verify function correctly validates passwords",
                "Utility has comprehensive unit tests",
            ],
            "priority": "high",
        },
        {
            "title": "Design API error responses",
            "type": "PAIR",
            "description": "Collaborate on standardized error response format for the API.",
            "acceptance_criteria": [
                "Error response includes code, message, details",
                "Documentation covers all error codes",
            ],
            "priority": "medium",
        },
    ],
    todos=[
        {"content": "Review existing codebase structure", "status": "completed"},
        {"content": "Identify dependencies and blockers", "status": "completed"},
        {"content": "Create prioritized task list", "status": "completed"},
    ],
)


PLAN_ACCEPTED_RESPONSE = """\
The plan has been accepted and tasks have been created.

The tasks are now in your backlog and ready to be started. I recommend \
beginning with the high-priority authentication task, as other features \
depend on it.

<complete/>
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
