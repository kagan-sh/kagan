from __future__ import annotations

import json
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock

import pytest

from kagan.wezterm import WeztermError, create_workspace_session, kill_workspace, workspace_exists

if TYPE_CHECKING:
    from pathlib import Path


async def test_workspace_exists_filters_by_workspace(monkeypatch: pytest.MonkeyPatch) -> None:
    payload = [
        {"workspace": "kagan-other", "pane_id": 11},
        {"workspace": "kagan-task-1", "pane_id": 22},
    ]
    run_mock = AsyncMock(return_value=json.dumps(payload))
    monkeypatch.setattr("kagan.wezterm.run_wezterm", run_mock)

    assert await workspace_exists("kagan-task-1") is True
    assert await workspace_exists("kagan-missing") is False


async def test_workspace_exists_raises_for_invalid_json(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr("kagan.wezterm.run_wezterm", AsyncMock(return_value="{"))

    with pytest.raises(WeztermError):
        await workspace_exists("kagan-task-1")


async def test_kill_workspace_kills_only_target_workspace_panes(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    payload = [
        {"workspace": "kagan-task-1", "pane_id": 1},
        {"workspace": "kagan-task-1", "pane_id": "2"},
        {"workspace": "kagan-other", "pane_id": 3},
    ]
    calls: list[tuple[str, ...]] = []

    async def fake_run(*args: str, **_kwargs: object) -> str:
        calls.append(args)
        if args[:3] == ("cli", "list", "--format"):
            return json.dumps(payload)
        return ""

    monkeypatch.setattr("kagan.wezterm.run_wezterm", fake_run)

    await kill_workspace("kagan-task-1")

    assert ("cli", "kill-pane", "--pane-id", "1") in calls
    assert ("cli", "kill-pane", "--pane-id", "2") in calls
    assert ("cli", "kill-pane", "--pane-id", "3") not in calls


async def test_create_workspace_session_passes_command_and_env(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    run_mock = AsyncMock(return_value="")
    monkeypatch.setattr("kagan.wezterm.run_wezterm", run_mock)

    await create_workspace_session(
        "kagan-task-1",
        tmp_path,
        env={"KAGAN_TASK_ID": "task-1"},
        command="echo hello",
    )

    await_args = run_mock.await_args
    assert await_args is not None
    args = await_args.args
    kwargs = await_args.kwargs
    assert args[:6] == (
        "start",
        "--always-new-process",
        "--workspace",
        "kagan-task-1",
        "--cwd",
        str(tmp_path),
    )
    assert "--" in args
    assert kwargs["env"] == {"KAGAN_TASK_ID": "task-1"}
