"""Tests for update CLI command."""

from __future__ import annotations

import shutil
import subprocess

import pytest
from click.testing import CliRunner

from kagan.cli.update import InstallationInfo, update

pytestmark = pytest.mark.integration


class TestUpdateCommand:
    """Tests for the update CLI command."""

    def test_update_check_mode_up_to_date(self, mocker):
        """Test --check flag when up to date."""
        mocker.patch("kagan.cli.update.get_installed_version", return_value="0.1.0")
        mocker.patch("kagan.cli.update.fetch_latest_version", return_value="0.1.0")

        runner = CliRunner()
        result = runner.invoke(update, ["--check"])

        assert result.exit_code == 0
        assert "latest version" in result.output.lower()

    def test_update_check_mode_update_available(self, mocker):
        """Test --check flag when update is available."""
        mocker.patch("kagan.cli.update.get_installed_version", return_value="1.0.0")
        mocker.patch("kagan.cli.update.fetch_latest_version", return_value="2.0.0")

        runner = CliRunner()
        result = runner.invoke(update, ["--check"])

        assert result.exit_code == 1
        assert "update available" in result.output.lower()

    def test_update_check_mode_error(self, mocker):
        """Test --check flag when error occurs."""
        mocker.patch("kagan.cli.update.get_installed_version", return_value="1.0.0")
        mocker.patch("kagan.cli.update.fetch_latest_version", return_value=None)

        runner = CliRunner()
        result = runner.invoke(update, ["--check"])

        assert result.exit_code == 2
        assert "error" in result.output.lower()

    def test_update_dev_version_warning(self, mocker):
        """Test warning shown for dev versions."""
        mocker.patch("kagan.cli.update.get_installed_version", return_value="dev")

        runner = CliRunner()
        result = runner.invoke(update)

        assert "development version" in result.output.lower()
        assert result.exit_code == 0

    def test_update_already_latest(self, mocker):
        """Test message when already on latest version."""
        mocker.patch("kagan.cli.update.get_installed_version", return_value="1.0.0")
        mocker.patch("kagan.cli.update.fetch_latest_version", return_value="1.0.0")

        runner = CliRunner()
        result = runner.invoke(update)

        assert result.exit_code == 0
        assert "already the latest version" in result.output.lower()

    def test_update_force_flag_skips_confirmation(self, mocker):
        """Test --force flag skips confirmation prompt."""
        mocker.patch("kagan.cli.update.get_installed_version", return_value="1.0.0")
        mocker.patch("kagan.cli.update.fetch_latest_version", return_value="2.0.0")
        mock_detect = mocker.patch("kagan.cli.update.detect_installation_method")
        mock_upgrade = mocker.patch("kagan.cli.update.run_upgrade")
        mock_detect.return_value = InstallationInfo(
            method="uv tool",
            upgrade_command=["uv", "tool", "upgrade", "kagan==2.0.0"],
        )
        mock_upgrade.return_value = (True, "Success")

        runner = CliRunner()
        result = runner.invoke(update, ["--force"])

        # Should not prompt, should call upgrade directly
        mock_upgrade.assert_called_once()
        assert "successfully upgraded" in result.output.lower()

    def test_update_user_declines(self, mocker):
        """Test user declining update prompt."""
        mocker.patch("kagan.cli.update.get_installed_version", return_value="1.0.0")
        mocker.patch("kagan.cli.update.fetch_latest_version", return_value="2.0.0")
        mock_detect = mocker.patch("kagan.cli.update.detect_installation_method")
        mock_detect.return_value = InstallationInfo(
            method="uv tool",
            upgrade_command=["uv", "tool", "upgrade", "kagan==2.0.0"],
        )

        runner = CliRunner()
        result = runner.invoke(update, input="n\n")

        assert "cancelled" in result.output.lower()

    def test_update_with_prerelease_flag(self, mocker):
        """Test --prerelease flag includes prerelease versions."""
        mocker.patch("kagan.cli.update.get_installed_version", return_value="1.0.0")
        # Mock fetch_latest_version to return prerelease when called with prerelease=True
        mock_fetch = mocker.patch("kagan.cli.update.fetch_latest_version")
        mock_fetch.return_value = "2.0.0b1"

        runner = CliRunner()
        result = runner.invoke(update, ["--check", "--prerelease"])

        assert result.exit_code == 1  # Update available
        assert "2.0.0b1" in result.output
        # Verify fetch was called with prerelease=True
        mock_fetch.assert_called_once_with(prerelease=True)

    def test_update_unknown_installation_shows_manual_instructions(self, mocker):
        """Test that unknown installation method shows manual upgrade instructions."""
        mocker.patch("kagan.cli.update.get_installed_version", return_value="1.0.0")
        mocker.patch("kagan.cli.update.fetch_latest_version", return_value="2.0.0")
        mocker.patch("kagan.cli.update.detect_installation_method", return_value=None)

        runner = CliRunner()
        result = runner.invoke(update, ["--force"])

        assert "could not detect" in result.output.lower()
        assert "uv tool upgrade" in result.output
        assert "pipx install" in result.output
        assert "pip install" in result.output


class TestUpgradeCommandSyntax:
    """Tests that validate upgrade command syntax against real tools.

    These tests actually invoke the package managers to verify that the
    command syntax we generate is valid and won't fail due to parsing errors.
    This catches issues like using `pkg@version` vs `pkg==version` syntax.
    """

    @pytest.mark.skipif(shutil.which("uv") is None, reason="uv not installed")
    def test_uv_tool_upgrade_command_syntax_is_valid(self):
        """Verify uv tool upgrade command uses valid version specifier syntax.

        The `uv tool upgrade` command requires `pkg==version` syntax (PEP 440),
        NOT `pkg@version` which is interpreted as a local path reference.

        This test uses a non-existent package to validate syntax parsing
        without actually performing an upgrade.
        """
        # Directly test the command pattern used for uv tool upgrades
        # Using a non-existent package ensures we test syntax, not actual upgrade
        test_command = ["uv", "tool", "upgrade", "nonexistent-test-pkg-kagan==1.0.0"]

        result = subprocess.run(
            test_command,
            capture_output=True,
            text=True,
        )

        # The command should fail because the package isn't installed,
        # NOT because of invalid syntax (path interpretation error)
        assert "Expected path" not in result.stderr, (
            f"uv interpreted version specifier as path. "
            f"Use `pkg==version` not `pkg@version`. stderr: {result.stderr}"
        )
        assert "file extension" not in result.stderr, (
            f"uv interpreted version specifier as path. "
            f"Use `pkg==version` not `pkg@version`. stderr: {result.stderr}"
        )
        # Valid syntax should produce "not installed" error
        assert "not installed" in result.stderr.lower() or "install" in result.stderr.lower()

    @pytest.mark.skipif(shutil.which("uv") is None, reason="uv not installed")
    def test_uv_tool_upgrade_at_syntax_is_invalid(self):
        """Verify that @ syntax fails with path interpretation error.

        This documents the incorrect syntax to prevent regression.
        """
        test_command = ["uv", "tool", "upgrade", "nonexistent-test-pkg-kagan@1.0.0"]

        result = subprocess.run(
            test_command,
            capture_output=True,
            text=True,
        )

        # @ syntax should fail with path interpretation error
        assert "Expected path" in result.stderr or "file extension" in result.stderr, (
            f"Expected @ syntax to fail with path error. stderr: {result.stderr}"
        )

    @pytest.mark.skipif(shutil.which("pipx") is None, reason="pipx not installed")
    def test_pipx_install_command_syntax_is_valid(self):
        """Verify pipx install command uses valid version specifier syntax."""
        # pipx uses `pkg==version` syntax with --force for upgrades
        test_command = ["pipx", "install", "nonexistent-test-pkg-kagan==1.0.0", "--force"]

        result = subprocess.run(
            test_command,
            capture_output=True,
            text=True,
        )

        # Should fail because package doesn't exist, not syntax error
        # pipx will try to fetch from PyPI and fail
        assert result.returncode != 0
        # Should not be a syntax/parsing error
        assert "invalid" not in result.stderr.lower() or "specifier" not in result.stderr.lower()
