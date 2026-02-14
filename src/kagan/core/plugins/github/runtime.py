"""Runtime helper functions for bundled GitHub plugin operations."""

from __future__ import annotations

from typing import Any

from kagan.core.plugins.github.contract import (
    GITHUB_CANONICAL_METHODS,
    GITHUB_CAPABILITY,
    GITHUB_CONTRACT_PROBE_METHOD,
    GITHUB_CONTRACT_VERSION,
    GITHUB_PLUGIN_ID,
    RESERVED_GITHUB_CAPABILITY,
)


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


__all__ = ["build_contract_probe_payload"]
