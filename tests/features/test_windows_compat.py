"""Windows compatibility tests for command lexing and preflight behavior."""

from __future__ import annotations

import sys
from typing import TYPE_CHECKING

import pytest

from kagan.ui.screens.troubleshooting.issue_presets import detect_issues
from kagan.utils.command_lex import join_args, quote_arg, split_command

if TYPE_CHECKING:
    from pytest import MonkeyPatch


@pytest.mark.unit
def test_split_command_uses_mslex_on_windows(monkeypatch: MonkeyPatch) -> None:
    """Use mslex parsing rules on Windows when available."""

    class FakeMslex:
        @staticmethod
        def split(command: str) -> list[str]:
            return ["MSLEX", command]

        @staticmethod
        def quote(value: str) -> str:
            return f"<{value}>"

        @staticmethod
        def join(args: list[str]) -> str:
            return "|".join(args)

    monkeypatch.setattr("platform.system", lambda: "Windows")
    monkeypatch.setitem(sys.modules, "mslex", FakeMslex)  # type: ignore[bad-argument-type]

    assert split_command("opencode --prompt hello") == ["MSLEX", "opencode --prompt hello"]
    assert quote_arg("hello world") == "<hello world>"
    assert join_args(["a", "b c"]) == "a|b c"


@pytest.mark.unit
@pytest.mark.asyncio
async def test_windows_preflight_is_warning_only(monkeypatch: MonkeyPatch) -> None:
    """Windows should not be a blocking preflight issue."""
    monkeypatch.setattr("platform.system", lambda: "Windows")

    result = await detect_issues(check_git=False, check_terminal=False)

    assert result.issues
    windows_issue = next(i for i in result.issues if i.preset.type.value == "windows_os")
    assert windows_issue.preset.severity.value == "warning"
    assert result.has_blocking_issues is False
