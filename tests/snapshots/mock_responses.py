"""Canned ACP responses for snapshot testing.

These mock responses simulate realistic agent output for different scenarios,
triggering the expected UI states without running actual AI.
"""

from __future__ import annotations

from typing import Any

# =============================================================================
# Plan Proposal Responses
# =============================================================================


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


# Pre-built plan proposal response (triggers plan approval UI)
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


# Multi-task plan proposal for complex features
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


# =============================================================================
# Plan Acceptance Response
# =============================================================================

PLAN_ACCEPTED_RESPONSE = """\
The plan has been accepted and tasks have been created.

The tasks are now in your backlog and ready to be started. I recommend \
beginning with the high-priority authentication task, as other features \
depend on it.

<complete/>
"""


# =============================================================================
# Task Completion Responses
# =============================================================================

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

TASK_COMPLETE_WITH_FILES_RESPONSE = """\
Implementation complete. Here's a summary of the changes:

### Files Created
- `src/models/user.py` - User model with validation
- `src/services/user_service.py` - Business logic for user operations
- `tests/test_user_service.py` - Unit tests for the service

### Files Modified
- `src/database/schema.sql` - Added users table
- `src/app.py` - Registered new routes

All tests pass and the implementation meets the acceptance criteria.

<complete/>
"""


# =============================================================================
# Task Continuation Response
# =============================================================================

TASK_CONTINUE_RESPONSE = """\
I've made progress on the task but need to continue.

## Completed So Far

- Set up the project structure
- Implemented the core data models
- Created initial database migrations

## Next Steps

- Implement the API endpoints
- Add validation logic
- Write tests

<continue/>
"""


# =============================================================================
# Task Blocked Response
# =============================================================================

TASK_BLOCKED_RESPONSE = """\
I've encountered an issue that prevents me from completing this task.

The acceptance criteria require database access, but the database \
connection configuration is missing from the environment.

<blocked reason="Missing DATABASE_URL environment variable"/>
"""

TASK_BLOCKED_DEPENDENCY_RESPONSE = """\
This task is blocked by a dependency issue.

The feature requires the authentication system to be implemented first, \
but the auth module doesn't exist yet.

<blocked reason="Depends on authentication module (not yet implemented)"/>
"""


# =============================================================================
# Clarification Responses
# =============================================================================

TASK_NEEDS_CLARIFICATION_RESPONSE = """\
I need some clarification before proceeding with this task.

The acceptance criteria mention "user preferences" but don't specify:

1. What preferences should be configurable?
2. Should preferences persist across sessions?
3. Are there default values for each preference?

Could you provide more details on these points?
"""

TASK_NEEDS_CLARIFICATION_SCOPE_RESPONSE = """\
Before I begin, I'd like to clarify the scope of this task.

The description mentions "improve performance" but doesn't specify:

- Which operations should be optimized?
- What is the current baseline performance?
- What target performance metrics should be achieved?

Please provide additional context so I can focus on the right areas.
"""


# =============================================================================
# Review Responses
# =============================================================================

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

REVIEW_APPROVE_WITH_NOTES_RESPONSE = """\
Approved with minor suggestions for future consideration.

## Review Notes

The implementation is solid and meets requirements. For future iterations:
- Consider adding rate limiting to the login endpoint
- The error messages could be more user-friendly

These are non-blocking suggestions.

<approve summary="Approved with suggestions for future improvements" \
approach="Standard REST patterns with input validation" \
key_files="src/routes/auth.py, src/middleware/rate_limit.py"/>
"""


# =============================================================================
# Review Rejection Responses
# =============================================================================

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

REVIEW_REJECT_TESTS_RESPONSE = """\
The implementation needs more test coverage before approval.

## Missing Tests

- No tests for edge cases (empty input, invalid format)
- No integration tests for the API endpoints
- Error paths are not tested

<reject reason="Insufficient test coverage - edge cases and error paths not tested"/>
"""

REVIEW_REJECT_CRITERIA_RESPONSE = """\
Some acceptance criteria are not met.

## Unmet Criteria

- "Token expiry is handled correctly" - No expiry check in the middleware
- "Logout invalidates the current token" - Logout endpoint not implemented

Please complete these requirements before requesting another review.

<reject reason="Acceptance criteria not fully met: missing token expiry and logout"/>
"""


# =============================================================================
# Helper Functions for Building Custom Responses
# =============================================================================


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
