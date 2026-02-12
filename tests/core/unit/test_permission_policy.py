from __future__ import annotations

import pytest

from kagan.core.security import CapabilityProfile
from kagan.core.services.permission_policy import (
    AgentPermissionScope,
    PermissionDecisionReason,
    resolve_auto_approve,
    resolve_mcp_capability,
    resolve_permission_decision,
)


def test_resolve_auto_approve_planner_uses_config() -> None:
    assert (
        resolve_auto_approve(
            scope=AgentPermissionScope.PLANNER,
            planner_auto_approve=True,
        )
        is True
    )
    assert (
        resolve_auto_approve(
            scope=AgentPermissionScope.PLANNER,
            planner_auto_approve=False,
        )
        is False
    )


@pytest.mark.parametrize(
    "scope",
    [
        AgentPermissionScope.AUTOMATION_RUNNER,
        AgentPermissionScope.AUTOMATION_REVIEWER,
        AgentPermissionScope.PROMPT_REFINER,
    ],
)
def test_resolve_auto_approve_non_planner_scopes_are_always_true(
    scope: AgentPermissionScope,
) -> None:
    assert (
        resolve_auto_approve(
            scope=scope,
            planner_auto_approve=False,
        )
        is True
    )


def test_resolve_permission_decision_prefers_auto_approve_flag() -> None:
    decision = resolve_permission_decision(
        auto_approve_enabled=True,
        has_message_target=True,
    )
    assert decision.auto_approve is True
    assert decision.reason is PermissionDecisionReason.AUTO_APPROVE_ENABLED


def test_resolve_permission_decision_auto_approves_without_ui_target() -> None:
    decision = resolve_permission_decision(
        auto_approve_enabled=False,
        has_message_target=False,
    )
    assert decision.auto_approve is True
    assert decision.reason is PermissionDecisionReason.NO_MESSAGE_TARGET


def test_resolve_permission_decision_waits_for_user_when_ui_is_available() -> None:
    decision = resolve_permission_decision(
        auto_approve_enabled=False,
        has_message_target=True,
    )
    assert decision.auto_approve is False
    assert decision.reason is PermissionDecisionReason.WAIT_FOR_USER


@pytest.mark.parametrize(
    ("task_id", "read_only", "expected"),
    [
        ("", True, CapabilityProfile.PLANNER),
        ("TASK-101", True, CapabilityProfile.VIEWER),
        ("TASK-102", False, CapabilityProfile.PAIR_WORKER),
    ],
)
def test_resolve_mcp_capability(
    task_id: str,
    read_only: bool,
    expected: CapabilityProfile,
) -> None:
    capability = resolve_mcp_capability(task_id=task_id, read_only=read_only)
    assert capability is expected
