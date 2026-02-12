"""Centralized policy for agent permission and auto-approve behavior."""

from __future__ import annotations

from dataclasses import dataclass
from enum import StrEnum

from kagan.core.security import CapabilityProfile


class AgentPermissionScope(StrEnum):
    """Execution scope used to resolve auto-approve behavior."""

    PLANNER = "planner"
    AUTOMATION_RUNNER = "automation_runner"
    AUTOMATION_REVIEWER = "automation_reviewer"
    PROMPT_REFINER = "prompt_refiner"


class PermissionDecisionReason(StrEnum):
    """Reason metadata for ACP permission handling decisions."""

    AUTO_APPROVE_ENABLED = "auto_approve_enabled"
    NO_MESSAGE_TARGET = "no_message_target"
    WAIT_FOR_USER = "wait_for_user"


@dataclass(frozen=True, slots=True)
class PermissionDecision:
    """Permission prompt behavior for an ACP permission request."""

    auto_approve: bool
    reason: PermissionDecisionReason


def resolve_auto_approve(*, scope: AgentPermissionScope, planner_auto_approve: bool) -> bool:
    """Resolve whether a scope should auto-approve ACP permission requests."""
    if scope is AgentPermissionScope.PLANNER:
        return planner_auto_approve
    return True


def resolve_permission_decision(
    *,
    auto_approve_enabled: bool,
    has_message_target: bool,
) -> PermissionDecision:
    """Resolve ACP permission behavior for the current runtime context."""
    if auto_approve_enabled:
        return PermissionDecision(
            auto_approve=True,
            reason=PermissionDecisionReason.AUTO_APPROVE_ENABLED,
        )
    if not has_message_target:
        return PermissionDecision(
            auto_approve=True,
            reason=PermissionDecisionReason.NO_MESSAGE_TARGET,
        )
    return PermissionDecision(
        auto_approve=False,
        reason=PermissionDecisionReason.WAIT_FOR_USER,
    )


def resolve_mcp_capability(*, task_id: str, read_only: bool) -> CapabilityProfile:
    """Resolve MCP capability profile for an ACP-backed agent session.

    Semantics:
    - planner entrypoint (read-only, unscoped) gets planner capabilities.
    - task-scoped read-only sessions remain viewer-only.
    - task-scoped writable sessions get pair_worker capabilities.
    - all other unscoped sessions default to viewer.
    """
    normalized_task_id = task_id.strip()
    if read_only and not normalized_task_id:
        return CapabilityProfile.PLANNER
    if read_only:
        return CapabilityProfile.VIEWER
    if normalized_task_id:
        return CapabilityProfile.PAIR_WORKER
    return CapabilityProfile.VIEWER


__all__ = [
    "AgentPermissionScope",
    "PermissionDecision",
    "PermissionDecisionReason",
    "resolve_auto_approve",
    "resolve_mcp_capability",
    "resolve_permission_decision",
]
