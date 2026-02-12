"""Canned ACP responses for planner and project scenarios.

Provides the ``make_propose_plan_tool_call`` builder and pre-built
plan proposal / multi-task plan constants used by snapshot and E2E tests.
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


# ---------------------------------------------------------------------------
# Pre-built plan response constants
# ---------------------------------------------------------------------------

PLAN_PROPOSAL_RESPONSE = """\
I've analyzed your request and created a development plan.

Let me propose a structured approach to implement this feature.
"""

SIMPLE_PLAN_TEXT = """\
I've created a plan for this change.
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
