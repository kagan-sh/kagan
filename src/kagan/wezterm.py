"""wezterm helpers for session management."""

from __future__ import annotations

import asyncio
import json
import os
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path


class WeztermError(RuntimeError):
    """Raised when wezterm commands fail."""

    pass


async def run_wezterm(*args: str, env: Mapping[str, str] | None = None) -> str:
    """Run a wezterm command and return stdout."""
    merged_env = None
    if env:
        merged_env = {**os.environ, **env}
    try:
        process = await asyncio.create_subprocess_exec(
            "wezterm",
            *args,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=merged_env,
        )
    except FileNotFoundError as exc:
        raise WeztermError("wezterm executable not found") from exc
    stdout, stderr = await process.communicate()
    if process.returncode != 0:
        stderr_text = stderr.decode().strip()
        cmd = " ".join(["wezterm", *args])
        detail = stderr_text or f"exit code {process.returncode}"
        raise WeztermError(f"{cmd} failed: {detail}")
    return stdout.decode().strip()


def _parse_wezterm_list_output(output: str) -> list[dict[str, Any]]:
    try:
        data = json.loads(output or "[]")
    except json.JSONDecodeError as exc:
        raise WeztermError("wezterm cli list returned invalid JSON") from exc
    if not isinstance(data, list):
        raise WeztermError("wezterm cli list returned non-list payload")
    return [row for row in data if isinstance(row, dict)]


def _workspace_pane_ids(rows: list[dict[str, Any]], workspace: str) -> list[str]:
    pane_ids: list[str] = []
    for row in rows:
        if row.get("workspace") != workspace:
            continue
        pane_id = row.get("pane_id")
        if isinstance(pane_id, int):
            pane_ids.append(str(pane_id))
        elif isinstance(pane_id, str) and pane_id:
            pane_ids.append(pane_id)
    return pane_ids


def _shell_command(command: str) -> list[str]:
    if os.name == "nt":
        return ["cmd.exe", "/c", command]
    return [os.environ.get("SHELL", "/bin/sh"), "-lc", command]


async def create_workspace_session(
    workspace: str,
    cwd: Path,
    *,
    env: Mapping[str, str] | None = None,
    command: str | None = None,
) -> None:
    """Create a detached wezterm workspace session."""
    args = [
        "start",
        "--always-new-process",
        "--workspace",
        workspace,
        "--cwd",
        str(cwd),
    ]
    if command:
        args.extend(["--", *_shell_command(command)])
    await run_wezterm(*args, env=env)


async def workspace_exists(workspace: str) -> bool:
    """Check if any pane exists in the workspace."""
    output = await run_wezterm("cli", "list", "--format", "json")
    rows = _parse_wezterm_list_output(output)
    return bool(_workspace_pane_ids(rows, workspace))


async def kill_workspace(workspace: str) -> None:
    """Kill all panes in a workspace."""
    output = await run_wezterm("cli", "list", "--format", "json")
    rows = _parse_wezterm_list_output(output)
    pane_ids = _workspace_pane_ids(rows, workspace)
    for pane_id in pane_ids:
        await run_wezterm("cli", "kill-pane", "--pane-id", pane_id)
