"""CLI tests for `kagan core` command group."""

from __future__ import annotations

from unittest.mock import patch

from click.testing import CliRunner

from kagan.cli.commands.core import core
from kagan.core.ipc.discovery import CoreEndpoint


def test_core_start_reports_existing_core() -> None:
    """`kagan core start` should not restart when a live endpoint exists."""
    runner = CliRunner()
    endpoint = CoreEndpoint(transport="tcp", address="127.0.0.1", port=4444, pid=1234)
    with (
        patch("kagan.core.ipc.discovery.discover_core_endpoint", return_value=endpoint),
        patch("kagan.core.launcher.ensure_core_running_sync") as mock_ensure,
    ):
        result = runner.invoke(core, ["start"])

    assert result.exit_code == 0
    assert "Core is already running." in result.output
    mock_ensure.assert_not_called()


def test_core_start_autostarts_when_not_running() -> None:
    """`kagan core start` should auto-start daemon and print endpoint details."""
    runner = CliRunner()
    endpoint = CoreEndpoint(transport="tcp", address="127.0.0.1", port=5555, pid=4321)
    with (
        patch("kagan.core.ipc.discovery.discover_core_endpoint", return_value=None),
        patch("kagan.core.launcher.ensure_core_running_sync", return_value=endpoint) as mock_ensure,
    ):
        result = runner.invoke(core, ["start"])

    assert result.exit_code == 0
    assert "Core started." in result.output
    assert "127.0.0.1" in result.output
    mock_ensure.assert_called_once()


def test_core_start_foreground_uses_blocking_launcher() -> None:
    """`kagan core start --foreground` should run blocking core host launcher."""
    runner = CliRunner()
    with (
        patch("kagan.core.ipc.discovery.discover_core_endpoint", return_value=None),
        patch("kagan.core.launcher.launch_core_subprocess", return_value=0) as mock_launch,
    ):
        result = runner.invoke(core, ["start", "--foreground"])

    assert result.exit_code == 0
    mock_launch.assert_called_once()


def test_core_status_reports_incomplete_runtime_metadata() -> None:
    """`kagan core status` should explain metadata drift when PID fallback is alive."""
    runner = CliRunner()
    with (
        patch("kagan.core.ipc.discovery.discover_core_endpoint", return_value=None),
        patch("kagan.cli.commands.core._discover_running_pid_fallback", return_value=2468),
    ):
        result = runner.invoke(core, ["status"])

    assert result.exit_code == 2
    assert "runtime metadata is incomplete" in result.output
    assert "2468" in result.output


def test_core_stop_uses_pid_fallback_when_endpoint_is_missing() -> None:
    """`kagan core stop` should stop a live PID even when endpoint discovery fails."""
    runner = CliRunner()
    with (
        patch("kagan.core.ipc.discovery.discover_core_endpoint", return_value=None),
        patch("kagan.cli.commands.core._discover_running_pid_fallback", return_value=2468),
        patch("os.kill") as mock_kill,
    ):
        result = runner.invoke(core, ["stop"])

    assert result.exit_code == 0
    assert "Stop signal sent." in result.output
    mock_kill.assert_called_once()
