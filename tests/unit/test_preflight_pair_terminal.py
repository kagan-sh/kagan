from __future__ import annotations

import platform
import shutil
from typing import TYPE_CHECKING

import pytest

from kagan.preflight import IssueType, detect_issues

if TYPE_CHECKING:
    from collections.abc import Callable

_IS_WINDOWS = platform.system() == "Windows"


def _mock_which(mapping: dict[str, str | None]) -> Callable[[str | None], str | None]:
    def _which(cmd: str | None) -> str | None:
        if cmd is None:
            return None
        return mapping.get(cmd.lower())

    return _which


@pytest.mark.unit
@pytest.mark.skipif(_IS_WINDOWS, reason="On Windows tmux defaults are overridden to vscode/cursor")
async def test_detect_issues_defaults_to_tmux_backend(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(shutil, "which", _mock_which({}))

    result = await detect_issues(check_git=False, check_terminal=False)

    assert len(result.issues) == 1
    assert result.issues[0].preset.type == IssueType.TMUX_MISSING
    assert result.has_blocking_issues is False


@pytest.mark.unit
@pytest.mark.skipif(_IS_WINDOWS, reason="On Windows tmux defaults are overridden to vscode/cursor")
async def test_detect_issues_warns_for_tmux_only_when_tmux_selected(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        shutil, "which", _mock_which({"code": "/usr/bin/code", "code.exe": "/usr/bin/code"})
    )

    result = await detect_issues(
        check_git=False,
        check_terminal=False,
        pair_terminal_backend="tmux",
        default_pair_terminal_backend="vscode",
    )

    assert len(result.issues) == 1
    assert result.issues[0].preset.type == IssueType.TMUX_MISSING


@pytest.mark.unit
@pytest.mark.mock_platform_system("Windows")
async def test_detect_issues_windows_resolves_vscode_exe(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(
        shutil,
        "which",
        _mock_which({"code.exe": r"C:\Program Files\Microsoft VS Code\bin\code.exe"}),
    )

    result = await detect_issues(check_git=False, check_terminal=False)

    assert result.issues == []


@pytest.mark.unit
@pytest.mark.mock_platform_system("Windows")
async def test_detect_issues_windows_missing_vscode_and_cursor_has_no_pair_issue(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(shutil, "which", _mock_which({}))

    result = await detect_issues(check_git=False, check_terminal=False)

    assert result.issues == []


@pytest.mark.unit
@pytest.mark.mock_platform_system("Windows")
async def test_detect_issues_windows_tmux_prefers_vscode_cursor_fallback(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(shutil, "which", _mock_which({}))

    result = await detect_issues(
        check_git=False,
        check_terminal=False,
        pair_terminal_backend="tmux",
    )

    assert result.issues == []
