"""Session startup bundle helpers for PAIR launchers."""

from __future__ import annotations

import asyncio
import json
from typing import TYPE_CHECKING

from kagan.core.models.enums import PairTerminalBackend, coerce_pair_backend

if TYPE_CHECKING:
    from pathlib import Path

SESSION_BUNDLE_DIR = ".kagan"
SESSION_BUNDLE_JSON = "session.json"
SESSION_BUNDLE_PROMPT = "start_prompt.md"
_EXTERNAL_LAUNCHER_BINARIES: dict[PairTerminalBackend, str] = {
    PairTerminalBackend.VSCODE: "code",
    PairTerminalBackend.CURSOR: "cursor",
}


def bundle_dir(worktree_path: Path) -> Path:
    return worktree_path / SESSION_BUNDLE_DIR


def bundle_prompt_path(worktree_path: Path) -> Path:
    return bundle_dir(worktree_path) / SESSION_BUNDLE_PROMPT


def bundle_json_path(worktree_path: Path) -> Path:
    return bundle_dir(worktree_path) / SESSION_BUNDLE_JSON


async def write_startup_bundle(
    *,
    task_id: str,
    worktree_path: Path,
    session_name: str,
    backend: str,
    startup_prompt: str,
) -> None:
    startup_bundle_dir = bundle_dir(worktree_path)
    await asyncio.to_thread(startup_bundle_dir.mkdir, parents=True, exist_ok=True)

    prompt_file = bundle_prompt_path(worktree_path)
    await asyncio.to_thread(prompt_file.write_text, startup_prompt, "utf-8")

    session_file = bundle_json_path(worktree_path)
    payload = {
        "task_id": task_id,
        "session_name": session_name,
        "backend": backend,
        "worktree": str(worktree_path),
        "prompt_file": str(prompt_file),
    }
    await asyncio.to_thread(session_file.write_text, json.dumps(payload, indent=2), "utf-8")


def build_external_launcher_command(backend: str, worktree_path: Path) -> list[str]:
    prompt_file = bundle_prompt_path(worktree_path)
    normalized_backend = coerce_pair_backend(backend)
    if normalized_backend is None:
        msg = f"Unsupported external PAIR launcher: {backend}"
        raise RuntimeError(msg)

    backend_kind = PairTerminalBackend(normalized_backend)
    binary = _EXTERNAL_LAUNCHER_BINARIES.get(backend_kind)
    if binary is not None:
        return [binary, "--new-window", str(worktree_path), str(prompt_file)]

    msg = f"Unsupported external PAIR launcher: {backend}"
    raise RuntimeError(msg)


__all__ = [
    "build_external_launcher_command",
    "bundle_dir",
    "bundle_json_path",
    "bundle_prompt_path",
    "write_startup_bundle",
]
