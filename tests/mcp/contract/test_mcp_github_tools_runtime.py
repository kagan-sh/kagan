"""Runtime wiring tests for GitHub MCP V1 tools."""

from __future__ import annotations

from typing import Any

from kagan.mcp.server import MCPRuntimeConfig, _create_mcp_server


def _tool(mcp: object, name: str):
    tool_manager = mcp._tool_manager  # type: ignore[attr-defined]  # quality-allow-private
    return tool_manager._tools[name]  # type: ignore[attr-defined]  # quality-allow-private


def _as_dict(value: object) -> dict[str, Any]:
    model_dump = getattr(value, "model_dump", None)
    if callable(model_dump):
        dumped = model_dump()
        if isinstance(dumped, dict):
            return dumped
    if isinstance(value, dict):
        return value
    msg = f"Unexpected tool response type: {type(value)!r}"
    raise TypeError(msg)


async def test_kagan_github_connect_repo_forwards_to_bridge_and_returns_contract_fields(
    monkeypatch,
) -> None:
    class _BridgeStub:
        def __init__(self) -> None:
            self.called_with: tuple[str, str, dict[str, Any] | None] | None = None

        async def invoke_plugin(
            self,
            capability: str,
            method: str,
            params: dict[str, Any] | None = None,
        ) -> dict[str, object]:
            self.called_with = (capability, method, params)
            return {
                "success": True,
                "code": "CONNECTED",
                "message": "Connected to acme/widgets",
                "plugin_id": "github",
                "connection": {
                    "full_name": "acme/widgets",
                    "owner": "acme",
                    "repo": "widgets",
                    "default_branch": "main",
                    "visibility": "PUBLIC",
                    "connected_at": "2026-02-10T12:00:00Z",
                },
            }

    bridge = _BridgeStub()
    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: bridge)
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(capability_profile="maintainer", identity="kagan_admin"),
    )
    tool = _tool(mcp, "kagan_github_connect_repo")

    result = await tool.fn(project_id="project-1", repo_id="repo-1", ctx=None)
    payload = _as_dict(result)

    assert bridge.called_with is not None
    cap, method, params = bridge.called_with
    assert cap == "kagan_github"
    assert method == "connect_repo"
    assert params == {"project_id": "project-1", "repo_id": "repo-1"}

    assert payload["success"] is True
    assert payload["message"] == "Connected to acme/widgets"
    assert payload["data"]["connection"] is not None
    assert payload["data"]["connection"]["owner"] == "acme"
    assert payload["data"]["connection"]["repo"] == "widgets"
    assert payload["data"]["connection"]["default_branch"] == "main"


async def test_kagan_github_sync_issues_propagates_core_error_code_and_message(
    monkeypatch,
) -> None:
    class _BridgeStub:
        async def invoke_plugin(
            self,
            capability: str,
            method: str,
            params: dict[str, Any] | None = None,
        ) -> dict[str, object]:
            assert capability == "kagan_github"
            assert method == "sync_issues"
            assert params is not None
            assert params["project_id"] == "project-1"
            assert params["repo_id"] == "repo-1"
            return {
                "success": False,
                "code": "GH_SYNC_FAILED",
                "message": "Failed to fetch issues: 401 Unauthorized",
                "plugin_id": "github",
                "hint": "Check gh CLI authentication and repository access",
                "stats": {
                    "total": 3,
                    "inserted": 0,
                    "updated": 0,
                    "reopened": 0,
                    "closed": 0,
                    "no_change": 0,
                    "errors": 3,
                },
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(capability_profile="maintainer", identity="kagan_admin"),
    )
    tool = _tool(mcp, "kagan_github_sync_issues")

    result = await tool.fn(project_id="project-1", repo_id="repo-1", ctx=None)
    payload = _as_dict(result)

    assert payload["success"] is False
    assert payload["message"] == "Failed to fetch issues: 401 Unauthorized"
    assert payload["data"]["hint"] == "Check gh CLI authentication and repository access"
    assert payload["data"]["stats"]["errors"] == 3
