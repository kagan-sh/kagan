"""Tests for MCP v2 tool catalog -- all expected tools are registered."""

from __future__ import annotations

from kagan.mcp.server import MCPRuntimeConfig, _create_mcp_server


def _tool_names(mcp) -> set[str]:
    """Extract registered tool names from a FastMCP instance."""
    return set(mcp._tool_manager._tools.keys())  # type: ignore[attr-defined]  # quality-allow-private


READONLY_TOOLS = {
    "propose_plan",
    "get_task",
    "tasks_list",
    "projects_list",
    "repos_list",
    "audit_tail",
}

FULL_TOOLS = {
    # original
    "get_task",
    "get_context",
    "update_scratchpad",
    "request_review",
    # v2 read-only
    "tasks_list",
    "projects_list",
    "repos_list",
    "audit_tail",
    "settings_get",
    "jobs_get",
    "jobs_wait",
    "jobs_events",
    "jobs_list_actions",
    # v2 mutating
    "tasks_create",
    "tasks_update",
    "tasks_move",
    "jobs_submit",
    "jobs_cancel",
    "sessions_create",
    "sessions_exists",
    "sessions_kill",
    "tasks_delete",
    "settings_update",
    "projects_create",
    "projects_open",
    "review",
}


def test_readonly_catalog_matches_expected_contract() -> None:
    names = _tool_names(_create_mcp_server(readonly=True))
    assert names == READONLY_TOOLS, (
        f"Tool mismatch.\nMissing: {READONLY_TOOLS - names}\nExtra: {names - READONLY_TOOLS}"
    )


def test_full_catalog_matches_expected_contract_without_instrumentation() -> None:
    names = _tool_names(_create_mcp_server(readonly=False))
    assert names == FULL_TOOLS, (
        f"Tool mismatch.\nMissing: {FULL_TOOLS - names}\nExtra: {names - FULL_TOOLS}"
    )
    assert "diagnostics_instrumentation" not in names


def test_full_catalog_can_enable_internal_instrumentation_for_maintainer() -> None:
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(
            capability_profile="maintainer",
            identity="kagan_admin",
            enable_internal_instrumentation=True,
        ),
    )
    names = _tool_names(mcp)
    assert names == FULL_TOOLS | {"diagnostics_instrumentation"}


def test_instrumentation_tool_requires_maintainer_capability() -> None:
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(
            capability_profile="viewer",
            identity="kagan",
            enable_internal_instrumentation=True,
        ),
    )
    names = _tool_names(mcp)
    assert "diagnostics_instrumentation" not in names
    assert names < FULL_TOOLS
