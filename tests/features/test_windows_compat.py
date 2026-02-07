"""Windows compatibility tests for command lexing and preflight behavior."""

from __future__ import annotations

import sys
from typing import TYPE_CHECKING

import pytest

from kagan.command_utils import split_command_string
from kagan.preflight import detect_issues

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
    monkeypatch.setattr("kagan.command_utils.is_windows", lambda: True)
    monkeypatch.setitem(sys.modules, "mslex", FakeMslex)  # type: ignore[bad-argument-type]

    assert split_command_string("opencode --prompt hello") == ["MSLEX", "opencode --prompt hello"]


@pytest.mark.unit
@pytest.mark.asyncio
async def test_windows_preflight_is_warning_only(monkeypatch: MonkeyPatch) -> None:
    """Windows should not be a blocking preflight issue."""
    monkeypatch.setattr("platform.system", lambda: "Windows")

    result = await detect_issues(check_git=False, check_terminal=False)

    assert result.has_blocking_issues is False
