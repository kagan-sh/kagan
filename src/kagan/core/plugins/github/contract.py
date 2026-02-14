"""Canonical contract constants for the bundled GitHub plugin."""

from __future__ import annotations

from typing import Final

GITHUB_PLUGIN_ID: Final = "official.github"
GITHUB_CAPABILITY: Final = "kagan_github"
GITHUB_CONTRACT_PROBE_METHOD: Final = "contract_probe"
GITHUB_METHOD_CONNECT_REPO: Final = "connect_repo"
GITHUB_METHOD_SYNC_ISSUES: Final = "sync_issues"
GITHUB_METHOD_ACQUIRE_LEASE: Final = "acquire_lease"
GITHUB_METHOD_RELEASE_LEASE: Final = "release_lease"
GITHUB_METHOD_GET_LEASE_STATE: Final = "get_lease_state"
GITHUB_METHOD_CREATE_PR_FOR_TASK: Final = "create_pr_for_task"
GITHUB_METHOD_LINK_PR_TO_TASK: Final = "link_pr_to_task"
GITHUB_METHOD_RECONCILE_PR_STATUS: Final = "reconcile_pr_status"
GITHUB_METHOD_VALIDATE_REVIEW_TRANSITION: Final = "validate_review_transition"
GITHUB_CONTRACT_VERSION: Final = "1.0.0"
RESERVED_GITHUB_CAPABILITY: Final = "github"

# Scope marker for canonical method discovery payloads.
# These methods represent the plugin capability surface, not the MCP V1
# admin tool subset.
GITHUB_CANONICAL_METHODS_SCOPE: Final = "plugin_capability"

GITHUB_CANONICAL_METHODS: Final[tuple[str, ...]] = (
    "connect_repo",
    "sync_issues",
    "acquire_lease",
    "release_lease",
    "get_lease_state",
    "create_pr_for_task",
    "link_pr_to_task",
    "reconcile_pr_status",
)

__all__ = [
    "GITHUB_CANONICAL_METHODS",
    "GITHUB_CANONICAL_METHODS_SCOPE",
    "GITHUB_CAPABILITY",
    "GITHUB_CONTRACT_PROBE_METHOD",
    "GITHUB_CONTRACT_VERSION",
    "GITHUB_METHOD_ACQUIRE_LEASE",
    "GITHUB_METHOD_CONNECT_REPO",
    "GITHUB_METHOD_CREATE_PR_FOR_TASK",
    "GITHUB_METHOD_GET_LEASE_STATE",
    "GITHUB_METHOD_LINK_PR_TO_TASK",
    "GITHUB_METHOD_RECONCILE_PR_STATUS",
    "GITHUB_METHOD_RELEASE_LEASE",
    "GITHUB_METHOD_SYNC_ISSUES",
    "GITHUB_METHOD_VALIDATE_REVIEW_TRANSITION",
    "GITHUB_PLUGIN_ID",
    "RESERVED_GITHUB_CAPABILITY",
]
