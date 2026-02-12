"""Canned ACP responses for snapshot testing.

This package re-exports all public symbols from its submodules so that
existing ``from tests.helpers.mock_responses import ...`` imports
continue to work without modification.
"""

from tests.helpers.mock_responses.projects import (
    MULTI_TASK_PLAN_TOOL_CALLS,
    PLAN_ACCEPTED_RESPONSE,
    PLAN_PROPOSAL_RESPONSE,
    PLAN_PROPOSAL_TOOL_CALLS,
    SIMPLE_PLAN_TEXT,
    make_propose_plan_tool_call,
)
from tests.helpers.mock_responses.tasks import (
    REVIEW_APPROVE_RESPONSE,
    REVIEW_APPROVE_SIMPLE_RESPONSE,
    REVIEW_REJECT_RESPONSE,
    SIMPLE_IMPLEMENTATION_TEXT,
    SIMPLE_REVIEW_APPROVE_TEXT,
    TASK_BLOCKED_RESPONSE,
    TASK_COMPLETE_RESPONSE,
    make_approve_response,
    make_blocked_response,
    make_clarification_response,
    make_complete_response,
    make_reject_response,
)

__all__ = [
    "MULTI_TASK_PLAN_TOOL_CALLS",
    "PLAN_ACCEPTED_RESPONSE",
    "PLAN_PROPOSAL_RESPONSE",
    "PLAN_PROPOSAL_TOOL_CALLS",
    "REVIEW_APPROVE_RESPONSE",
    "REVIEW_APPROVE_SIMPLE_RESPONSE",
    "REVIEW_REJECT_RESPONSE",
    "SIMPLE_IMPLEMENTATION_TEXT",
    "SIMPLE_PLAN_TEXT",
    "SIMPLE_REVIEW_APPROVE_TEXT",
    "TASK_BLOCKED_RESPONSE",
    "TASK_COMPLETE_RESPONSE",
    "make_approve_response",
    "make_blocked_response",
    "make_clarification_response",
    "make_complete_response",
    "make_propose_plan_tool_call",
    "make_reject_response",
]
