"""Canonical contract constants for the bundled GitHub plugin."""

from __future__ import annotations

from typing import Final

GITHUB_PLUGIN_ID: Final = "official.github"
GITHUB_CAPABILITY: Final = "kagan_github"
GITHUB_CONTRACT_PROBE_METHOD: Final = "contract_probe"
GITHUB_CONTRACT_VERSION: Final = "1.0.0"
RESERVED_GITHUB_CAPABILITY: Final = "github"

GITHUB_CANONICAL_METHODS: Final[tuple[str, ...]] = (
    "connect_repo",
    "sync_issues",
    "create_pr_for_task",
    "link_pr_to_task",
    "reconcile_pr_status",
    "repair_task_issue_mapping",
)

__all__ = [
    "GITHUB_CANONICAL_METHODS",
    "GITHUB_CAPABILITY",
    "GITHUB_CONTRACT_PROBE_METHOD",
    "GITHUB_CONTRACT_VERSION",
    "GITHUB_PLUGIN_ID",
    "RESERVED_GITHUB_CAPABILITY",
]
