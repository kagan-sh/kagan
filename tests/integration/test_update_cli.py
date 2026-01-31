"""Tests for update CLI command."""

from __future__ import annotations

import pytest
from click.testing import CliRunner

from kagan.cli.update import InstallationInfo, UpdateCheckResult, update

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
            upgrade_command=["uv", "tool", "upgrade", "kagan@2.0.0"],
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
            upgrade_command=["uv", "tool", "upgrade", "kagan@2.0.0"],
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


class TestFetchLatestVersion:
    """Tests for fetch_latest_version function with actual HTTP mocking."""

    def test_fetch_latest_version_stable(self, httpx_mock):
        """Test fetching stable version from PyPI."""
        from kagan.cli.update import fetch_latest_version

        httpx_mock.add_response(
            url="https://pypi.org/pypi/kagan/json",
            json={"info": {"version": "1.2.3"}, "releases": {}},
        )

        result = fetch_latest_version(prerelease=False)
        assert result == "1.2.3"

    def test_fetch_latest_version_prerelease(self, httpx_mock):
        """Test fetching prerelease version from PyPI."""
        from kagan.cli.update import fetch_latest_version

        httpx_mock.add_response(
            url="https://pypi.org/pypi/kagan/json",
            json={
                "info": {"version": "1.0.0"},
                "releases": {"1.0.0": [], "2.0.0b1": [], "1.5.0": []},
            },
        )

        result = fetch_latest_version(prerelease=True)
        assert result == "2.0.0b1"

    def test_fetch_latest_version_timeout(self, httpx_mock):
        """Test fetch returns None on timeout."""
        import httpx as httpx_lib

        from kagan.cli.update import fetch_latest_version

        httpx_mock.add_exception(httpx_lib.TimeoutException("Timeout"))

        result = fetch_latest_version()
        assert result is None

    def test_fetch_latest_version_http_error(self, httpx_mock):
        """Test fetch returns None on HTTP error."""
        from kagan.cli.update import fetch_latest_version

        httpx_mock.add_response(
            url="https://pypi.org/pypi/kagan/json",
            status_code=500,
        )

        result = fetch_latest_version()
        assert result is None


class TestCheckForUpdates:
    """Tests for check_for_updates function."""

    def test_check_for_updates_with_update_available(self, mocker):
        """Test check_for_updates returns correct result when update available."""
        from kagan.cli.update import check_for_updates

        mocker.patch("kagan.cli.update.get_installed_version", return_value="1.0.0")
        mocker.patch("kagan.cli.update.fetch_latest_version", return_value="2.0.0")

        result = check_for_updates()

        assert result.current_version == "1.0.0"
        assert result.latest_version == "2.0.0"
        assert result.update_available is True
        assert result.is_dev is False

    def test_check_for_updates_dev_version(self, mocker):
        """Test check_for_updates handles dev version correctly."""
        from kagan.cli.update import check_for_updates

        mocker.patch("kagan.cli.update.get_installed_version", return_value="dev")

        result = check_for_updates()

        assert result.is_dev is True
        assert result.update_available is False


class TestUpdateCheckResult:
    """Tests for UpdateCheckResult dataclass."""

    def test_update_available_when_newer(self):
        """Test update_available returns True when newer version exists."""
        result = UpdateCheckResult(
            current_version="1.0.0",
            latest_version="2.0.0",
            is_dev=False,
        )
        assert result.update_available is True

    def test_update_available_when_same(self):
        """Test update_available returns False when versions match."""
        result = UpdateCheckResult(
            current_version="1.0.0",
            latest_version="1.0.0",
            is_dev=False,
        )
        assert result.update_available is False

    def test_update_available_when_dev(self):
        """Test update_available returns False for dev versions."""
        result = UpdateCheckResult(
            current_version="dev",
            latest_version="2.0.0",
            is_dev=True,
        )
        assert result.update_available is False

    def test_update_available_when_no_latest(self):
        """Test update_available returns False when latest is None."""
        result = UpdateCheckResult(
            current_version="1.0.0",
            latest_version=None,
            is_dev=False,
        )
        assert result.update_available is False
