"""Tests for version detection and update checking."""

from __future__ import annotations

import httpx
import pytest

from kagan.cli.update import (
    UpdateCheckResult,
    check_for_updates,
    fetch_latest_version,
    is_dev_version,
)

pytestmark = pytest.mark.integration


class TestVersionDetection:
    """Tests for version detection utilities."""

    def test_is_dev_version_dev_string(self):
        """Test that 'dev' is recognized as dev version."""
        assert is_dev_version("dev") is True

    def test_is_dev_version_with_dev_suffix(self):
        """Test that versions with .dev suffix are recognized."""
        assert is_dev_version("0.1.0.dev1") is True
        assert is_dev_version("1.0.0.dev0") is True

    def test_is_dev_version_editable(self):
        """Test that editable installs are recognized."""
        assert is_dev_version("0.1.0+editable") is True

    def test_is_dev_version_normal(self):
        """Test that normal versions are not dev versions."""
        assert is_dev_version("0.1.0") is False
        assert is_dev_version("1.2.3") is False
        assert is_dev_version("0.1.0a1") is False  # alpha is not dev
        assert is_dev_version("0.1.0b2") is False  # beta is not dev


class TestFetchLatestVersion:
    """Tests for fetching latest version from PyPI."""

    def test_fetch_latest_version_success(self, httpx_mock):
        """Test successful version fetch from PyPI."""
        httpx_mock.add_response(
            url="https://pypi.org/pypi/kagan/json",
            json={
                "info": {"version": "1.0.0"},
                "releases": {"0.1.0": [], "1.0.0": []},
            },
        )

        result = fetch_latest_version()
        assert result == "1.0.0"

    def test_fetch_latest_version_with_prerelease(self, httpx_mock):
        """Test fetching latest version including prereleases."""
        httpx_mock.add_response(
            url="https://pypi.org/pypi/kagan/json",
            json={
                "info": {"version": "1.0.0"},
                "releases": {"0.1.0": [], "1.0.0": [], "2.0.0b1": []},
            },
        )

        result = fetch_latest_version(prerelease=True)
        assert result == "2.0.0b1"

    def test_fetch_latest_version_prerelease_false_ignores_prerelease(self, httpx_mock):
        """Test that prereleases are ignored when prerelease=False."""
        httpx_mock.add_response(
            url="https://pypi.org/pypi/kagan/json",
            json={
                "info": {"version": "1.0.0"},
                "releases": {"0.1.0": [], "1.0.0": [], "2.0.0b1": []},
            },
        )

        result = fetch_latest_version(prerelease=False)
        assert result == "1.0.0"

    def test_fetch_latest_version_timeout(self, httpx_mock):
        """Test handling of timeout errors."""
        httpx_mock.add_exception(httpx.TimeoutException("Connection timed out"))

        result = fetch_latest_version()
        assert result is None

    def test_fetch_latest_version_http_error(self, httpx_mock):
        """Test handling of HTTP errors."""
        httpx_mock.add_response(
            url="https://pypi.org/pypi/kagan/json",
            status_code=500,
        )

        result = fetch_latest_version()
        assert result is None

    def test_fetch_latest_version_network_error(self, httpx_mock):
        """Test handling of network errors."""
        httpx_mock.add_exception(httpx.ConnectError("Connection refused"))

        result = fetch_latest_version()
        assert result is None


class TestCheckForUpdates:
    """Tests for the check_for_updates function."""

    def test_check_for_updates_dev_version(self, mocker, httpx_mock):
        """Test that dev versions skip update check."""
        mocker.patch("kagan.cli.update.get_installed_version", return_value="dev")
        result = check_for_updates()

        assert result.is_dev is True
        assert result.update_available is False
        assert result.error == "Running from development version"

    def test_check_for_updates_update_available(self, mocker, httpx_mock):
        """Test detection of available updates."""
        httpx_mock.add_response(
            url="https://pypi.org/pypi/kagan/json",
            json={"info": {"version": "2.0.0"}, "releases": {}},
        )

        mocker.patch("kagan.cli.update.get_installed_version", return_value="1.0.0")
        result = check_for_updates()

        assert result.is_dev is False
        assert result.current_version == "1.0.0"
        assert result.latest_version == "2.0.0"
        assert result.update_available is True

    def test_check_for_updates_already_latest(self, mocker, httpx_mock):
        """Test when already on latest version."""
        httpx_mock.add_response(
            url="https://pypi.org/pypi/kagan/json",
            json={"info": {"version": "1.0.0"}, "releases": {}},
        )

        mocker.patch("kagan.cli.update.get_installed_version", return_value="1.0.0")
        result = check_for_updates()

        assert result.update_available is False
        assert result.current_version == "1.0.0"
        assert result.latest_version == "1.0.0"

    def test_check_for_updates_fetch_failure(self, mocker, httpx_mock):
        """Test handling of fetch failures."""
        httpx_mock.add_exception(httpx.TimeoutException("Timeout"))

        mocker.patch("kagan.cli.update.get_installed_version", return_value="1.0.0")
        result = check_for_updates()

        assert result.update_available is False
        assert result.error == "Failed to fetch version from PyPI"
        assert result.latest_version is None


class TestUpdateCheckResult:
    """Tests for UpdateCheckResult properties."""

    def test_update_available_newer_version(self):
        """Test update_available with newer version."""
        result = UpdateCheckResult(
            current_version="1.0.0",
            latest_version="2.0.0",
            is_dev=False,
        )
        assert result.update_available is True

    def test_update_available_same_version(self):
        """Test update_available with same version."""
        result = UpdateCheckResult(
            current_version="1.0.0",
            latest_version="1.0.0",
            is_dev=False,
        )
        assert result.update_available is False

    def test_update_available_dev_version(self):
        """Test update_available with dev version."""
        result = UpdateCheckResult(
            current_version="dev",
            latest_version="2.0.0",
            is_dev=True,
        )
        assert result.update_available is False

    def test_update_available_no_latest(self):
        """Test update_available when latest is None."""
        result = UpdateCheckResult(
            current_version="1.0.0",
            latest_version=None,
            is_dev=False,
        )
        assert result.update_available is False

    def test_update_available_prerelease_comparison(self):
        """Test update_available with prerelease versions."""
        # Prerelease is newer than stable
        result = UpdateCheckResult(
            current_version="1.0.0",
            latest_version="2.0.0b1",
            is_dev=False,
        )
        assert result.update_available is True

        # Same version, prerelease is older than stable
        result = UpdateCheckResult(
            current_version="1.0.0",
            latest_version="1.0.0b1",
            is_dev=False,
        )
        assert result.update_available is False
