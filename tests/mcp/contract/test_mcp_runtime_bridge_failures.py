from __future__ import annotations

from unittest.mock import patch

import pytest

from kagan.mcp import server as mcp_server
from kagan.mcp.server import MCPRuntimeConfig, MCPStartupError, _create_mcp_server, _mcp_lifespan


def _tool(mcp: object, name: str):
    tool_manager = mcp._tool_manager  # type: ignore[attr-defined]  # quality-allow-private
    return tool_manager._tools[name]  # type: ignore[attr-defined]  # quality-allow-private


@pytest.mark.asyncio
async def test_server_resolve_endpoint_errors_when_autostart_disabled_and_missing() -> None:
    mcp = _create_mcp_server(readonly=True)

    with (
        patch.object(mcp_server, "_resolve_endpoint", return_value=None),
        patch.object(mcp_server, "_is_core_autostart_enabled", return_value=False),
    ):
        with pytest.raises(MCPStartupError) as exc_info:
            async with _mcp_lifespan(mcp, MCPRuntimeConfig()):
                pass

    assert exc_info.value.code == "NO_ENDPOINT"
    assert exc_info.value.message == "No active Kagan core endpoint was discovered."
    assert exc_info.value.hint == "Start Kagan or run `kagan core start`, then reconnect MCP."


@pytest.mark.asyncio
async def test_server_profile_update_rejects_invalid_types_with_invalid_params(monkeypatch) -> None:
    class _BridgeStub:
        async def update_settings(self, fields: dict[str, object]) -> dict[str, object]:
            # Tool forwards typed payload map and core rejects invalid values.
            assert fields["general.max_concurrent_agents"] == "not-a-number"
            return {
                "success": False,
                "code": "INVALID_PARAMS",
                "message": "max_concurrent_agents must be an integer",
                "hint": "Pass max_concurrent_agents as an integer value.",
                "updated": {},
                "settings": {},
            }

    monkeypatch.setattr("kagan.mcp.server._require_bridge", lambda _ctx: _BridgeStub())
    mcp = _create_mcp_server(
        readonly=False,
        runtime_config=MCPRuntimeConfig(capability_profile="maintainer", identity="kagan_admin"),
    )
    tool = _tool(mcp, "settings_set")

    result = await tool.fn(max_concurrent_agents="not-a-number", ctx=None)

    assert result.success is False
    assert result.code == "INVALID_PARAMS"
    assert result.message == "max_concurrent_agents must be an integer"
    assert result.hint == "Pass max_concurrent_agents as an integer value."


@pytest.mark.asyncio
async def test_bridge_requirements_fail_closed_with_contract_error_shape() -> None:
    mcp = _create_mcp_server(readonly=True)
    tool = _tool(mcp, "task_get")

    with pytest.raises(ValueError) as exc_info:
        await tool.fn(task_id="task-1", ctx=None)

    text = str(exc_info.value)
    assert text.startswith("[NO_CONTEXT]")
    assert "No active Kagan session." in text
