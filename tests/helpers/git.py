"""Git test utilities."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path


async def _run_git(repo_path: Path, *args: str) -> None:
    """Run git command silently."""
    proc = await asyncio.create_subprocess_exec(
        "git",
        *args,
        cwd=repo_path,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    await proc.communicate()


async def configure_git_user(repo_path: Path) -> None:
    """Configure git user for test repo (email, name, no gpg sign)."""
    for cmd in [
        ("config", "user.email", "test@test.com"),
        ("config", "user.name", "Test User"),
        ("config", "commit.gpgsign", "false"),
    ]:
        await _run_git(repo_path, *cmd)


async def init_git_repo_with_commit(
    path: Path,
    initial_file: str = "README.md",
    branch: str = "main",
) -> Path:
    """Initialize git repo with initial commit. Returns repo path."""
    await _run_git(path, "init", "-b", branch)
    await configure_git_user(path)

    (path / initial_file).write_text("# Test Repo\n")
    await _run_git(path, "add", ".")
    await _run_git(path, "commit", "-m", "Initial commit")

    return path
