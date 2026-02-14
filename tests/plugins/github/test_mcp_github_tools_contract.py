"""Contract tests for GitHub plugin MCP tools (V1 contract).

These tests verify the stable V1 contract for GitHub admin MCP tools:
- Tool names are frozen and follow kagan_github_* naming convention
- Tools are only available to MAINTAINER profile
- Parameter schemas are stable
- Response fields match documented contract

Note: These tests verify MCP registration and profile gating only.
Actual plugin runtime behavior is tested in plugin-specific tests.
"""

from __future__ import annotations

import pytest

from kagan.core.plugins.github.contract import GITHUB_CANONICAL_METHODS
from kagan.mcp.registrars import (
    GITHUB_MCP_V1_TOOLS,
    GITHUB_TOOL_CONNECT_REPO,
    GITHUB_TOOL_CONTRACT_PROBE,
    GITHUB_TOOL_SYNC_ISSUES,
)
from kagan.mcp.server import MCPRuntimeConfig, _create_mcp_server

# ---------------------------------------------------------------------------
# V1 Contract Constants (frozen)
# ---------------------------------------------------------------------------

# Tool names are part of the V1 contract and must not change
V1_GITHUB_TOOLS = frozenset(
    {
        GITHUB_TOOL_CONTRACT_PROBE,
        GITHUB_TOOL_CONNECT_REPO,
        GITHUB_TOOL_SYNC_ISSUES,
    }
)

# Expected tool names as literal strings for documentation
EXPECTED_TOOL_NAMES = frozenset(
    {
        "kagan_github_contract_probe",
        "kagan_github_connect_repo",
        "kagan_github_sync_issues",
    }
)


def _tool_names(mcp) -> set[str]:
    """Extract registered tool names from a FastMCP instance."""
    return set(mcp._tool_manager._tools.keys())  # type: ignore[attr-defined]


def _tool_schema(mcp, tool_name: str) -> dict | None:
    """Extract input schema for a tool."""
    tool = mcp._tool_manager._tools.get(tool_name)  # type: ignore[attr-defined]
    if tool is None:
        return None
    return tool.parameters


# ---------------------------------------------------------------------------
# V1 Contract: Tool Name Stability
# ---------------------------------------------------------------------------


def test_v1_tool_names_match_contract_constants() -> None:
    """Tool name constants match expected literal strings."""
    assert V1_GITHUB_TOOLS == EXPECTED_TOOL_NAMES


def test_v1_tool_names_follow_naming_convention() -> None:
    """All GitHub tools follow kagan_github_* naming convention."""
    for name in V1_GITHUB_TOOLS:
        assert name.startswith("kagan_github_"), f"Tool {name} must start with kagan_github_"


def test_mcp_v1_tools_are_documented_subset_of_plugin_capability_methods() -> None:
    """MCP V1 exposes an admin subset; canonical methods describe plugin capability surface."""
    assert frozenset(GITHUB_MCP_V1_TOOLS) == V1_GITHUB_TOOLS
    canonical = set(GITHUB_CANONICAL_METHODS)
    assert {"connect_repo", "sync_issues"} <= canonical
    assert "contract_probe" not in canonical


# ---------------------------------------------------------------------------
# V1 Contract: Profile Gating
# ---------------------------------------------------------------------------


def test_github_tools_require_maintainer_profile() -> None:
    """GitHub tools are only registered for MAINTAINER profile."""
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(
            capability_profile="maintainer",
            identity="kagan_admin",
        ),
    )
    names = _tool_names(mcp)
    assert names >= V1_GITHUB_TOOLS, f"Missing tools: {V1_GITHUB_TOOLS - names}"


@pytest.mark.parametrize(
    "profile",
    ["viewer", "planner", "pair_worker", "operator"],
)
def test_github_tools_not_available_to_non_maintainer_profiles(profile: str) -> None:
    """GitHub tools are not exposed to profiles below MAINTAINER."""
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(
            capability_profile=profile,
            identity="kagan",
        ),
    )
    names = _tool_names(mcp)
    intersection = V1_GITHUB_TOOLS & names
    assert not intersection, f"Profile {profile} should not have GitHub tools: {intersection}"


def test_github_tools_not_in_readonly_mode() -> None:
    """GitHub tools are not available in readonly mode."""
    mcp = _create_mcp_server(readonly=True)
    names = _tool_names(mcp)
    intersection = V1_GITHUB_TOOLS & names
    assert not intersection, f"Readonly mode should not have GitHub tools: {intersection}"


# ---------------------------------------------------------------------------
# V1 Contract: Parameter Schema Stability
# ---------------------------------------------------------------------------


def test_contract_probe_schema_is_stable() -> None:
    """kagan_github_contract_probe has stable parameter schema."""
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(
            capability_profile="maintainer",
            identity="kagan_admin",
        ),
    )
    schema = _tool_schema(mcp, GITHUB_TOOL_CONTRACT_PROBE)
    assert schema is not None

    # V1 contract: echo is the only parameter and it's optional
    props = schema.get("properties", {})
    assert "echo" in props, "echo parameter is part of V1 contract"

    required = schema.get("required", [])
    assert "echo" not in required, "echo should be optional"


def test_connect_repo_schema_is_stable() -> None:
    """kagan_github_connect_repo has stable parameter schema."""
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(
            capability_profile="maintainer",
            identity="kagan_admin",
        ),
    )
    schema = _tool_schema(mcp, GITHUB_TOOL_CONNECT_REPO)
    assert schema is not None

    props = schema.get("properties", {})
    # V1 contract: project_id is required, repo_id is optional
    assert "project_id" in props, "project_id parameter is part of V1 contract"
    assert "repo_id" in props, "repo_id parameter is part of V1 contract"

    required = schema.get("required", [])
    assert "project_id" in required, "project_id should be required"
    assert "repo_id" not in required, "repo_id should be optional"


def test_sync_issues_schema_is_stable() -> None:
    """kagan_github_sync_issues has stable parameter schema."""
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(
            capability_profile="maintainer",
            identity="kagan_admin",
        ),
    )
    schema = _tool_schema(mcp, GITHUB_TOOL_SYNC_ISSUES)
    assert schema is not None

    props = schema.get("properties", {})
    # V1 contract: project_id is required, repo_id is optional
    assert "project_id" in props, "project_id parameter is part of V1 contract"
    assert "repo_id" in props, "repo_id parameter is part of V1 contract"

    required = schema.get("required", [])
    assert "project_id" in required, "project_id should be required"
    assert "repo_id" not in required, "repo_id should be optional"


# ---------------------------------------------------------------------------
# V1 Contract: Tool Annotations
# ---------------------------------------------------------------------------


def test_contract_probe_is_read_only() -> None:
    """kagan_github_contract_probe is marked as read-only."""
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(
            capability_profile="maintainer",
            identity="kagan_admin",
        ),
    )
    tool = mcp._tool_manager._tools.get(GITHUB_TOOL_CONTRACT_PROBE)  # type: ignore[attr-defined]
    assert tool is not None
    assert tool.annotations is not None
    assert tool.annotations.readOnlyHint is True


def test_connect_repo_is_mutating() -> None:
    """kagan_github_connect_repo is marked as mutating."""
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(
            capability_profile="maintainer",
            identity="kagan_admin",
        ),
    )
    tool = mcp._tool_manager._tools.get(GITHUB_TOOL_CONNECT_REPO)  # type: ignore[attr-defined]
    assert tool is not None
    assert tool.annotations is not None
    assert tool.annotations.readOnlyHint is False


def test_sync_issues_is_mutating() -> None:
    """kagan_github_sync_issues is marked as mutating."""
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(
            capability_profile="maintainer",
            identity="kagan_admin",
        ),
    )
    tool = mcp._tool_manager._tools.get(GITHUB_TOOL_SYNC_ISSUES)  # type: ignore[attr-defined]
    assert tool is not None
    assert tool.annotations is not None
    assert tool.annotations.readOnlyHint is False
