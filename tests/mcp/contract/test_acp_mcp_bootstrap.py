"""Tests for ACP session MCP bootstrap args."""

from __future__ import annotations

from unittest.mock import patch

from kagan.core.acp.kagan_agent import _build_mcp_args
from kagan.core.ipc.discovery import CoreEndpoint
from kagan.core.services.sessions import _build_session_mcp_args

_DISCOVERY_PATH = "kagan.core.ipc.discovery.discover_core_endpoint"


# ---------------------------------------------------------------------------
# _build_mcp_args (KaganAgent ACP bootstrap)
# ---------------------------------------------------------------------------


class TestBuildMcpArgs:
    """Verify MCP args built for ACP agent sessions."""

    @patch(_DISCOVERY_PATH, return_value=None)
    def test_task_id_includes_session_and_capability(self, _mock: object) -> None:
        args = _build_mcp_args(task_id="TASK-001", read_only=False)

        assert "--session-id" in args
        assert args[args.index("--session-id") + 1] == "task:TASK-001"
        assert "--capability" in args
        assert args[args.index("--capability") + 1] == "pair_worker"
        assert "--identity" in args
        assert args[args.index("--identity") + 1] == "kagan"

    @patch(_DISCOVERY_PATH, return_value=None)
    def test_read_only_agent_gets_viewer_capability(self, _mock: object) -> None:
        args = _build_mcp_args(task_id="TASK-002", read_only=True)

        assert args[args.index("--capability") + 1] == "viewer"

    @patch(_DISCOVERY_PATH, return_value=None)
    def test_no_task_id_defaults_to_viewer(self, _mock: object) -> None:
        args = _build_mcp_args(task_id="", read_only=False)

        assert args[args.index("--capability") + 1] == "viewer"
        assert "--session-id" not in args

    @patch(_DISCOVERY_PATH, return_value=None)
    def test_read_only_without_task_id_uses_planner_capability(self, _mock: object) -> None:
        args = _build_mcp_args(task_id="", read_only=True)

        assert args[args.index("--capability") + 1] == "planner"
        assert "--session-id" not in args

    @patch(
        _DISCOVERY_PATH,
        return_value=CoreEndpoint(transport="tcp", address="127.0.0.1", port=9876),
    )
    def test_endpoint_discovery_propagated(self, _mock: object) -> None:
        args = _build_mcp_args(task_id="TASK-003", read_only=False)

        assert "--endpoint" in args
        assert args[args.index("--endpoint") + 1] == "127.0.0.1:9876"

    @patch(
        _DISCOVERY_PATH,
        return_value=CoreEndpoint(transport="socket", address="/tmp/kagan.sock"),
    )
    def test_endpoint_without_port(self, _mock: object) -> None:
        args = _build_mcp_args(task_id="TASK-004", read_only=False)

        assert "--endpoint" in args
        assert args[args.index("--endpoint") + 1] == "/tmp/kagan.sock"

    @patch(_DISCOVERY_PATH, return_value=None)
    def test_no_endpoint_omits_flag(self, _mock: object) -> None:
        args = _build_mcp_args(task_id="TASK-005", read_only=False)

        assert "--endpoint" not in args


# ---------------------------------------------------------------------------
# _build_session_mcp_args (SessionService config writer)
# ---------------------------------------------------------------------------


class TestBuildSessionMcpArgs:
    """Verify MCP args built for PAIR session configs."""

    @patch(_DISCOVERY_PATH, return_value=None)
    def test_includes_session_id_and_capability(self, _mock: object) -> None:
        args = _build_session_mcp_args("TASK-010")

        assert args[:2] == ["mcp", "--session-id"]
        assert args[2] == "task:TASK-010"
        assert "--capability" in args
        assert args[args.index("--capability") + 1] == "pair_worker"
        assert "--identity" in args
        assert args[args.index("--identity") + 1] == "kagan"

    @patch(_DISCOVERY_PATH, return_value=None)
    def test_custom_capability(self, _mock: object) -> None:
        args = _build_session_mcp_args("TASK-011", capability="planner")

        assert args[args.index("--capability") + 1] == "planner"

    @patch(
        _DISCOVERY_PATH,
        return_value=CoreEndpoint(transport="tcp", address="localhost", port=5555),
    )
    def test_endpoint_propagated(self, _mock: object) -> None:
        args = _build_session_mcp_args("TASK-012")

        assert "--endpoint" in args
        assert args[args.index("--endpoint") + 1] == "localhost:5555"

    @patch(_DISCOVERY_PATH, return_value=None)
    def test_no_endpoint_omits_flag(self, _mock: object) -> None:
        args = _build_session_mcp_args("TASK-013")

        assert "--endpoint" not in args
