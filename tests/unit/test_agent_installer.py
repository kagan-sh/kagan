"""Tests for the agent installer module."""

from __future__ import annotations

import asyncio
from unittest.mock import AsyncMock, MagicMock

import pytest

from kagan.agents.installer import (
    INSTALL_COMMAND,
    INSTALL_TIMEOUT_SECONDS,
    InstallerError,
    check_claude_code_installed,
    check_opencode_installed,
    get_install_command,
    install_agent,
    install_claude_code,
    install_opencode,
)

pytestmark = pytest.mark.unit


class TestGetInstallCommand:
    def test_returns_correct_command(self):
        assert get_install_command() == "curl -fsSL https://claude.ai/install.sh | sh"

    def test_returns_constant(self):
        assert get_install_command() == INSTALL_COMMAND


class TestCheckClaudeCodeInstalled:
    @pytest.mark.asyncio
    async def test_returns_true_when_claude_in_path(self, mocker):
        mocker.patch("shutil.which", return_value="/usr/local/bin/claude")
        assert await check_claude_code_installed() is True

    @pytest.mark.asyncio
    async def test_returns_false_when_claude_not_in_path(self, mocker):
        mocker.patch("shutil.which", return_value=None)
        assert await check_claude_code_installed() is False


class TestInstallClaudeCode:
    @pytest.mark.asyncio
    async def test_successful_installation(self, mocker):
        mock_proc = AsyncMock()
        mock_proc.returncode = 0
        mock_proc.communicate = AsyncMock(return_value=(b"Installed!\n", b""))
        mock_create = mocker.patch("asyncio.create_subprocess_shell", return_value=mock_proc)
        mocker.patch("kagan.agents.installer.check_claude_code_installed", return_value=True)
        success, message = await install_claude_code()
        assert success is True and "successfully" in message.lower()
        mock_create.assert_called_once_with(
            INSTALL_COMMAND, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE
        )

    @pytest.mark.asyncio
    async def test_installation_success_but_not_in_path(self, mocker):
        mock_proc = AsyncMock()
        mock_proc.returncode = 0
        mock_proc.communicate = AsyncMock(return_value=(b"Done\n", b""))
        mocker.patch("asyncio.create_subprocess_shell", return_value=mock_proc)
        mocker.patch("kagan.agents.installer.check_claude_code_installed", return_value=False)
        success, message = await install_claude_code()
        assert success is True and ("restart" in message.lower() or "PATH" in message)

    @pytest.mark.asyncio
    async def test_installation_failure_with_stderr(self, mocker):
        mock_proc = AsyncMock()
        mock_proc.returncode = 1
        mock_proc.communicate = AsyncMock(return_value=(b"", b"curl: (6) Could not resolve host\n"))
        mocker.patch("asyncio.create_subprocess_shell", return_value=mock_proc)
        success, message = await install_claude_code()
        assert (
            success is False and "failed" in message.lower() and "Could not resolve host" in message
        )

    @pytest.mark.asyncio
    async def test_installation_failure_with_exit_code_only(self, mocker):
        mock_proc = AsyncMock()
        mock_proc.returncode = 127
        mock_proc.communicate = AsyncMock(return_value=(b"", b""))
        mocker.patch("asyncio.create_subprocess_shell", return_value=mock_proc)
        success, message = await install_claude_code()
        assert success is False and "failed" in message.lower() and "127" in message

    @pytest.mark.asyncio
    async def test_installation_timeout(self, mocker):
        mock_proc = AsyncMock()
        mock_proc.kill = MagicMock()
        mock_proc.wait = AsyncMock()

        async def slow_communicate():
            await asyncio.sleep(10)
            return (b"", b"")

        mock_proc.communicate = slow_communicate
        mocker.patch("asyncio.create_subprocess_shell", return_value=mock_proc)
        success, message = await install_claude_code(timeout=0.1)
        assert success is False and "timed out" in message.lower()
        mock_proc.kill.assert_called_once()

    @pytest.mark.asyncio
    async def test_curl_not_found(self, mocker):
        mocker.patch(
            "asyncio.create_subprocess_shell", side_effect=FileNotFoundError("curl not found")
        )
        success, message = await install_claude_code()
        assert success is False and "curl" in message.lower() and "not found" in message.lower()

    @pytest.mark.asyncio
    async def test_os_error(self, mocker):
        mocker.patch("asyncio.create_subprocess_shell", side_effect=OSError("Permission denied"))
        success, message = await install_claude_code()
        assert success is False and "Permission denied" in message

    @pytest.mark.asyncio
    async def test_custom_timeout(self, mocker):
        mock_proc = AsyncMock()
        mock_proc.returncode = 0
        mock_proc.communicate = AsyncMock(return_value=(b"OK\n", b""))
        mocker.patch("asyncio.create_subprocess_shell", return_value=mock_proc)
        mocker.patch("kagan.agents.installer.check_claude_code_installed", return_value=True)
        success, _ = await install_claude_code(timeout=300)
        assert success is True

    @pytest.mark.asyncio
    async def test_default_timeout_value(self):
        assert INSTALL_TIMEOUT_SECONDS == 120


class TestInstallerError:
    def test_exception_can_be_raised(self):
        with pytest.raises(InstallerError):
            raise InstallerError("Test error")

    def test_exception_message(self):
        try:
            raise InstallerError("Custom error message")
        except InstallerError as e:
            assert str(e) == "Custom error message"


class TestCheckOpencodeInstalled:
    def test_returns_true_when_opencode_available(self, mocker):
        mocker.patch("shutil.which", return_value="/usr/bin/opencode")
        assert check_opencode_installed() is True

    def test_returns_false_when_opencode_missing(self, mocker):
        mocker.patch("shutil.which", return_value=None)
        assert check_opencode_installed() is False


class TestInstallOpencode:
    @pytest.mark.asyncio
    async def test_fails_when_npm_not_available(self, mocker):
        mocker.patch("shutil.which", return_value=None)
        success, message = await install_opencode()
        assert success is False and "npm" in message.lower()

    @pytest.mark.asyncio
    async def test_success_when_npm_install_works(self, mocker):
        mocker.patch("shutil.which", side_effect=lambda x: f"/usr/bin/{x}")
        mock_proc = AsyncMock()
        mock_proc.returncode = 0
        mock_proc.communicate = AsyncMock(return_value=(b"", b""))
        mocker.patch("asyncio.create_subprocess_shell", return_value=mock_proc)
        success, _ = await install_opencode()
        assert success is True

    @pytest.mark.asyncio
    async def test_fails_when_npm_install_fails(self, mocker):
        mocker.patch("shutil.which", side_effect=lambda x: "/usr/bin/npm" if x == "npm" else None)
        mock_proc = AsyncMock()
        mock_proc.returncode = 1
        mock_proc.communicate = AsyncMock(return_value=(b"", b"npm ERR!"))
        mocker.patch("asyncio.create_subprocess_shell", return_value=mock_proc)
        success, _ = await install_opencode()
        assert success is False


class TestInstallAgent:
    @pytest.mark.asyncio
    async def test_dispatches_to_claude_installer(self, mocker):
        mock = mocker.patch("kagan.agents.installer.install_claude_code", return_value=(True, "OK"))
        await install_agent("claude")
        mock.assert_called_once()

    @pytest.mark.asyncio
    async def test_dispatches_to_opencode_installer(self, mocker):
        mock = mocker.patch("kagan.agents.installer.install_opencode", return_value=(True, "OK"))
        await install_agent("opencode")
        mock.assert_called_once()

    @pytest.mark.asyncio
    async def test_raises_error_for_unknown_agent(self):
        with pytest.raises(ValueError, match="unknown"):
            await install_agent("unknown")


class TestGetInstallCommandWithAgent:
    def test_returns_claude_command_by_default(self):
        assert "claude" in get_install_command()

    def test_returns_claude_command_explicitly(self):
        assert "claude" in get_install_command("claude")

    def test_returns_opencode_command(self):
        cmd = get_install_command("opencode")
        assert "npm" in cmd and "opencode" in cmd

    def test_raises_error_for_unknown_agent(self):
        with pytest.raises(ValueError, match="unknown"):
            get_install_command("unknown")
