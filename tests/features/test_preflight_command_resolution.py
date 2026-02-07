"""Tests for ACP command resolution across platforms."""

from __future__ import annotations

import shutil
from typing import TYPE_CHECKING

import pytest

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

from kagan.preflight import IssueType, resolve_acp_command


def _mock_which(mapping: dict[str, str | None]) -> Callable[[str | None], str | None]:
    def _which(cmd: str | None) -> str | None:
        if cmd is None:
            return None
        return mapping.get(cmd.lower())

    return _which


@pytest.mark.unit
@pytest.mark.mock_platform_system("Windows")
@pytest.mark.parametrize(
    "npx_path",
    [r"C:\Program Files\nodejs\npx.cmd", r"C:\node\npx.cmd"],
)
def test_resolve_acp_command_uses_npx_cmd_when_available(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, npx_path: str
) -> None:
    monkeypatch.setenv("APPDATA", str(tmp_path))
    monkeypatch.setattr(
        shutil,
        "which",
        _mock_which(
            {
                "npx": npx_path,
                "npx.cmd": npx_path,
                "claude-code-acp": None,
            }
        ),
    )

    resolution = resolve_acp_command("npx claude-code-acp --flag")

    assert resolution.issue is None
    assert resolution.resolved_command is not None
    assert resolution.resolved_command[0] == npx_path
    assert resolution.resolved_command[1:] == ["claude-code-acp", "--flag"]
    assert (tmp_path / "npm").is_dir()


@pytest.mark.unit
@pytest.mark.mock_platform_system("Windows")
def test_resolve_acp_command_prefers_global_binary(monkeypatch: pytest.MonkeyPatch) -> None:
    binary_path = r"C:\Tools\claude-code-acp.exe"
    monkeypatch.setattr(
        shutil,
        "which",
        _mock_which(
            {
                "claude-code-acp": binary_path,
                "npx.cmd": None,
                "npx": None,
            }
        ),
    )

    resolution = resolve_acp_command("npx claude-code-acp --flag")

    assert resolution.issue is None
    assert resolution.used_fallback is True
    assert resolution.resolved_command == [binary_path, "--flag"]


@pytest.mark.unit
@pytest.mark.mock_platform_system("Windows")
def test_resolve_acp_command_missing_npx_reports_issue(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(shutil, "which", _mock_which({}))

    resolution = resolve_acp_command("npx claude-code-acp")

    assert resolution.resolved_command is None
    assert resolution.issue is not None
    assert resolution.issue.preset.type == IssueType.NPX_MISSING
