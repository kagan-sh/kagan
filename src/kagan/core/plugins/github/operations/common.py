"""Shared constants and payload helpers for GitHub plugin runtime operations."""

from __future__ import annotations

from typing import Any, Final

from kagan.core.plugins.github.contract import (
    GITHUB_CANONICAL_METHODS,
    GITHUB_CAPABILITY,
    GITHUB_CONTRACT_PROBE_METHOD,
    GITHUB_CONTRACT_VERSION,
    GITHUB_PLUGIN_ID,
    RESERVED_GITHUB_CAPABILITY,
)
from kagan.core.plugins.github.gh_adapter import (
    GH_PR_CREATE_FAILED,
    GH_PR_LINK_FAILED,
    GH_PR_NOT_FOUND,
)

GH_NOT_CONNECTED: Final = "GH_NOT_CONNECTED"
GH_SYNC_FAILED: Final = "GH_SYNC_FAILED"
GH_ISSUE_REQUIRED: Final = "GH_ISSUE_REQUIRED"
GH_TASK_REQUIRED: Final = "GH_TASK_REQUIRED"
GH_WORKSPACE_REQUIRED: Final = "GH_WORKSPACE_REQUIRED"


def build_contract_probe_payload(params: dict[str, Any]) -> dict[str, Any]:
    """Return a stable, machine-readable contract response for probe calls."""
    return {
        "success": True,
        "plugin_id": GITHUB_PLUGIN_ID,
        "contract_version": GITHUB_CONTRACT_VERSION,
        "capability": GITHUB_CAPABILITY,
        "method": GITHUB_CONTRACT_PROBE_METHOD,
        "canonical_methods": list(GITHUB_CANONICAL_METHODS),
        "reserved_official_capability": RESERVED_GITHUB_CAPABILITY,
        "echo": params.get("echo"),
    }


__all__ = [
    "GH_ISSUE_REQUIRED",
    "GH_NOT_CONNECTED",
    "GH_PR_CREATE_FAILED",
    "GH_PR_LINK_FAILED",
    "GH_PR_NOT_FOUND",
    "GH_SYNC_FAILED",
    "GH_TASK_REQUIRED",
    "GH_WORKSPACE_REQUIRED",
    "build_contract_probe_payload",
]
