"""Tests for TUI update check gate and startup integration."""

from __future__ import annotations

import pytest
from click.testing import CliRunner

from kagan.cli.update import UpdateCheckResult

pytestmark = pytest.mark.integration


class TestCheckForUpdatesGate:
    """Tests for the TUI startup version check gate."""

    def test_gate_skips_for_dev_version(self, mocker, httpx_mock):
        """Test that gate silently skips for dev versions."""
        from kagan.__main__ import _check_for_updates_gate

        mock_check = mocker.patch("kagan.__main__.check_for_updates")
        mock_check.return_value = UpdateCheckResult(
            current_version="dev",
            latest_version=None,
            is_dev=True,
            error="Running from development version",
        )

        # Should not raise and not prompt
        _check_for_updates_gate()

    def test_gate_skips_on_fetch_error(self, mocker, httpx_mock):
        """Test that gate silently skips on fetch errors."""
        from kagan.__main__ import _check_for_updates_gate

        mock_check = mocker.patch("kagan.__main__.check_for_updates")
        mock_check.return_value = UpdateCheckResult(
            current_version="1.0.0",
            latest_version=None,
            is_dev=False,
            error="Failed to fetch version from PyPI",
        )

        # Should not raise and not prompt
        _check_for_updates_gate()

    def test_gate_no_update_available(self, mocker, httpx_mock):
        """Test that gate does nothing when no update available."""
        from kagan.__main__ import _check_for_updates_gate

        mock_check = mocker.patch("kagan.__main__.check_for_updates")
        mock_check.return_value = UpdateCheckResult(
            current_version="1.0.0",
            latest_version="1.0.0",
            is_dev=False,
        )

        # Should not raise
        _check_for_updates_gate()

    def test_gate_prompts_when_update_available_user_declines(self, mocker, httpx_mock):
        """Test that gate prompts user when update available and user declines."""
        from kagan.__main__ import _check_for_updates_gate

        mock_check = mocker.patch("kagan.__main__.check_for_updates")
        mocker.patch("kagan.__main__.click.confirm", return_value=False)
        mocker.patch("kagan.__main__.click.echo")
        mocker.patch("kagan.__main__.click.secho")
        mock_check.return_value = UpdateCheckResult(
            current_version="1.0.0",
            latest_version="2.0.0",
            is_dev=False,
        )

        # Should not raise, user declined
        _check_for_updates_gate()

    def test_gate_prompts_when_update_available_user_accepts(self, mocker, httpx_mock):
        """Test that gate prompts user and exits after successful update."""
        from kagan.__main__ import _check_for_updates_gate

        mock_check = mocker.patch("kagan.__main__.check_for_updates")
        mocker.patch("kagan.__main__.click.confirm", return_value=True)
        mock_update = mocker.patch("kagan.__main__.prompt_and_update", return_value=True)
        mocker.patch("kagan.__main__.click.echo")
        mocker.patch("kagan.__main__.click.secho")
        mock_exit = mocker.patch("kagan.__main__.sys.exit")
        mock_check.return_value = UpdateCheckResult(
            current_version="1.0.0",
            latest_version="2.0.0",
            is_dev=False,
        )

        _check_for_updates_gate()

        # Should have called prompt_and_update with force=True
        mock_update.assert_called_once()
        # Should exit after successful update
        mock_exit.assert_called_once_with(0)


class TestTuiCommandWithUpdateCheck:
    """Tests for the tui command with update check integration."""

    def test_tui_skip_update_check_flag(self, mocker):
        """Test that --skip-update-check flag skips update check."""
        from kagan.__main__ import cli

        runner = CliRunner()
        mock_gate = mocker.patch("kagan.__main__._check_for_updates_gate")
        mocker.patch("kagan.__main__.Path.exists", return_value=False)
        # Mock lock to prevent actual lock acquisition
        mocker.patch("kagan.lock.InstanceLock.acquire")
        mocker.patch("kagan.lock.InstanceLock.release")
        # Mock app to prevent TUI from actually running
        mocker.patch("kagan.app.KaganApp.run")

        result = runner.invoke(cli, ["tui", "--skip-update-check", "--skip-preflight"])
        assert result.exit_code == 0

        # Gate should not have been called
        mock_gate.assert_not_called()

    def test_tui_env_var_skips_update_check(self, mocker, monkeypatch):
        """Test that KAGAN_SKIP_UPDATE_CHECK env var skips update check."""
        from kagan.__main__ import cli

        monkeypatch.setenv("KAGAN_SKIP_UPDATE_CHECK", "1")

        runner = CliRunner()
        mock_gate = mocker.patch("kagan.__main__._check_for_updates_gate")
        mocker.patch("kagan.__main__.Path.exists", return_value=False)
        mocker.patch("kagan.lock.InstanceLock.acquire")
        mocker.patch("kagan.lock.InstanceLock.release")
        mocker.patch("kagan.app.KaganApp.run")

        result = runner.invoke(cli, ["tui", "--skip-preflight"])
        assert result.exit_code == 0

        # Gate should not have been called
        mock_gate.assert_not_called()
