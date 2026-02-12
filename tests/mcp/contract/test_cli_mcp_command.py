"""CLI tests for the `kagan mcp` command."""

from __future__ import annotations

from unittest.mock import patch

from click.testing import CliRunner

from kagan.cli.commands.mcp import mcp


def test_mcp_command_forwards_capability() -> None:
    """`kagan mcp --capability` is forwarded to MCP server main()."""
    runner = CliRunner()
    with patch("kagan.mcp.server.main") as mock_main:
        result = runner.invoke(
            mcp,
            [
                "--session-id",
                "TASK-123",
                "--capability",
                "pair_worker",
                "--endpoint",
                "/tmp/core.sock",
                "--identity",
                "kagan",
            ],
        )
    assert result.exit_code == 0
    mock_main.assert_called_once_with(
        readonly=False,
        endpoint="/tmp/core.sock",
        session_id="TASK-123",
        capability="pair_worker",
        identity="kagan",
    )


def test_mcp_command_rejects_invalid_capability() -> None:
    """Invalid capability value returns a CLI error without starting MCP server."""
    runner = CliRunner()
    result = runner.invoke(mcp, ["--capability", "invalid-profile"])
    assert result.exit_code != 0
    assert "Invalid capability" in result.output


def test_mcp_command_rejects_invalid_identity() -> None:
    """Invalid identity value returns a CLI error without starting MCP server."""
    runner = CliRunner()
    result = runner.invoke(mcp, ["--identity", "unknown-lane"])
    assert result.exit_code != 0
    assert "Invalid identity" in result.output


def test_mcp_command_forwards_internal_instrumentation_flag() -> None:
    """`kagan mcp --enable-internal-instrumentation` is forwarded to server main()."""
    runner = CliRunner()
    with patch("kagan.mcp.server.main") as mock_main:
        result = runner.invoke(mcp, ["--enable-internal-instrumentation"])

    assert result.exit_code == 0
    mock_main.assert_called_once_with(
        readonly=False,
        endpoint=None,
        session_id=None,
        capability=None,
        identity=None,
        enable_internal_instrumentation=True,
    )


def test_mcp_command_backcompat_when_server_main_lacks_new_flag() -> None:
    """CLI should degrade gracefully when kagan-mcp is older than kagan-app."""
    runner = CliRunner()
    captured: dict[str, object] = {}

    def _legacy_main(
        *,
        readonly: bool = False,
        endpoint: str | None = None,
        session_id: str | None = None,
        capability: str | None = None,
        identity: str | None = None,
    ) -> None:
        captured.update(
            {
                "readonly": readonly,
                "endpoint": endpoint,
                "session_id": session_id,
                "capability": capability,
                "identity": identity,
            }
        )

    with patch("kagan.mcp.server.main", new=_legacy_main):
        result = runner.invoke(mcp, ["--enable-internal-instrumentation"])

    assert result.exit_code == 0
    assert captured == {
        "readonly": False,
        "endpoint": None,
        "session_id": None,
        "capability": None,
        "identity": None,
    }
