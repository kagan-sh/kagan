"""Tests for installation detection and upgrade execution."""

from __future__ import annotations

from pathlib import Path

import pytest

from kagan.cli.update import (
    InstallationInfo,
    UpdateCheckResult,
    detect_installation_method,
    prompt_and_update,
    run_upgrade,
)

pytestmark = pytest.mark.unit


class TestDetectInstallationMethod:
    """Tests for installation method detection."""

    def test_detect_uv_tool_installation(self, mocker):
        """Test detection of uv tool installation."""
        mocker.patch(
            "kagan.cli.update._get_installer_info",
            return_value=("uv", Path("/home/user/.local/share/uv/tools/kagan")),
        )

        result = detect_installation_method("2.0.0")

        assert result is not None
        assert result.method == "uv tool"
        assert result.upgrade_command == ["uv", "tool", "upgrade", "kagan@2.0.0"]

    def test_detect_pipx_installation(self, mocker):
        """Test detection of pipx installation."""
        mocker.patch(
            "kagan.cli.update._get_installer_info",
            return_value=("pipx", Path("/home/user/.local/pipx/venvs/kagan")),
        )

        result = detect_installation_method("2.0.0")

        assert result is not None
        assert result.method == "pipx"
        assert "pipx" in result.upgrade_command[0]

    def test_detect_pip_installation(self, mocker):
        """Test detection of pip installation."""
        import sys

        mocker.patch(
            "kagan.cli.update._get_installer_info",
            return_value=("pip", Path("/home/user/venv/lib/python3.12/site-packages")),
        )
        mocker.patch.object(Path, "exists", return_value=True)

        result = detect_installation_method("2.0.0")

        assert result is not None
        assert result.method == "pip"
        assert sys.executable in result.upgrade_command[0]

    def test_detect_unknown_installation(self, mocker):
        """Test handling of unknown installation method."""
        mocker.patch("kagan.cli.update._get_installer_info", return_value=None)

        result = detect_installation_method("2.0.0")

        assert result is None


class TestRunUpgrade:
    """Tests for running upgrade commands."""

    def test_run_upgrade_success(self):
        """Test successful upgrade execution."""
        info = InstallationInfo(
            method="uv tool",
            upgrade_command=["echo", "success"],
        )

        success, message = run_upgrade(info)

        assert success is True
        assert "successfully" in message.lower()

    def test_run_upgrade_command_fails(self):
        """Test handling of failed upgrade command."""
        info = InstallationInfo(
            method="uv tool",
            upgrade_command=["false"],  # Command that always fails
        )

        success, message = run_upgrade(info)

        assert success is False
        assert "failed" in message.lower()

    def test_run_upgrade_command_not_found(self):
        """Test handling of command not found."""
        info = InstallationInfo(
            method="uv tool",
            upgrade_command=["nonexistent_command_xyz", "arg"],
        )

        success, message = run_upgrade(info)

        assert success is False
        assert "not found" in message.lower()


class TestPromptAndUpdate:
    """Tests for the prompt_and_update function."""

    def test_prompt_and_update_no_update_available(self):
        """Test that no action is taken when no update available."""
        result = UpdateCheckResult(
            current_version="1.0.0",
            latest_version="1.0.0",
            is_dev=False,
        )

        updated = prompt_and_update(result)
        assert updated is False

    def test_prompt_and_update_force_mode(self, httpx_mock, mocker):
        """Test force mode skips confirmation."""
        result = UpdateCheckResult(
            current_version="1.0.0",
            latest_version="2.0.0",
            is_dev=False,
        )

        mock_detect = mocker.patch("kagan.cli.update.detect_installation_method")
        mock_upgrade = mocker.patch("kagan.cli.update.run_upgrade")

        mock_detect.return_value = InstallationInfo(
            method="uv tool",
            upgrade_command=["uv", "tool", "upgrade", "kagan@2.0.0"],
        )
        mock_upgrade.return_value = (True, "Success")

        updated = prompt_and_update(result, force=True)

        assert updated is True
        mock_upgrade.assert_called_once()


class TestInstallationInfo:
    """Tests for InstallationInfo dataclass."""

    def test_format_command(self):
        """Test formatting upgrade command as string."""
        info = InstallationInfo(
            method="uv tool",
            upgrade_command=["uv", "tool", "upgrade", "kagan@2.0.0"],
        )

        assert info.format_command() == "uv tool upgrade kagan@2.0.0"

    def test_format_command_with_pip(self):
        """Test formatting pip upgrade command."""
        info = InstallationInfo(
            method="pip",
            upgrade_command=["/usr/bin/python", "-m", "pip", "install", "kagan==2.0.0"],
        )

        assert info.format_command() == "/usr/bin/python -m pip install kagan==2.0.0"
