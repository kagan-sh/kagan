from __future__ import annotations

from unittest.mock import AsyncMock, patch

import pytest

from kagan.core.command_utils import clear_which_cache
from kagan.tui.terminals.installer import check_terminal_installed, install_terminal


@pytest.fixture(autouse=True)
def _clear_cache() -> None:
    """Ensure cached_which cache is empty before every test."""
    clear_which_cache()


class _Proc:
    def __init__(self, *, returncode: int, stdout: bytes = b"", stderr: bytes = b"") -> None:
        self.returncode = returncode
        self._stdout = stdout
        self._stderr = stderr

    async def communicate(self) -> tuple[bytes, bytes]:
        return self._stdout, self._stderr

    def kill(self) -> None:
        return None

    async def wait(self) -> int:
        return self.returncode


@pytest.mark.asyncio
async def test_install_terminal_returns_manual_fallback_when_auto_installer_unavailable() -> None:
    with (
        patch("kagan.tui.terminals.installer.check_terminal_installed", return_value=False),
        patch("kagan.tui.terminals.installer._get_tmux_install_command", return_value=None),
    ):
        success, message = await install_terminal("tmux")

    assert success is False
    assert "install tmux" in message.lower()


@pytest.mark.asyncio
async def test_install_terminal_surfaces_command_failure_with_fallback() -> None:
    with (
        patch("kagan.tui.terminals.installer._get_tmux_install_command", return_value="install"),
        patch("kagan.tui.terminals.installer.check_terminal_installed", return_value=False),
        patch(
            "kagan.tui.terminals.installer.asyncio.create_subprocess_shell",
            new=AsyncMock(return_value=_Proc(returncode=1, stderr=b"boom")),
        ),
    ):
        success, message = await install_terminal("tmux")

    assert success is False
    assert "boom" in message
    assert "install tmux" in message.lower()


@pytest.mark.asyncio
async def test_install_terminal_supports_tmux_auto_install() -> None:
    with (
        patch("kagan.tui.terminals.installer._get_tmux_install_command", return_value="install"),
        patch(
            "kagan.tui.terminals.installer.check_terminal_installed",
            side_effect=[False, True],
        ),
        patch(
            "kagan.tui.terminals.installer.asyncio.create_subprocess_shell",
            new=AsyncMock(return_value=_Proc(returncode=0)),
        ),
    ):
        success, message = await install_terminal("tmux")

    assert success is True
    assert "installed" in message.lower()


@pytest.mark.asyncio
async def test_install_terminal_rejects_non_tmux_backend() -> None:
    success, message = await install_terminal("vscode")
    assert success is False
    assert "only for tmux" in message.lower()


def test_check_terminal_installed_supports_vscode_and_cursor(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with patch(
        "shutil.which",
        side_effect=lambda cmd: "/bin/x" if cmd in {"code", "cursor"} else None,
    ):
        assert check_terminal_installed("vscode") is True
        assert check_terminal_installed("cursor") is True
