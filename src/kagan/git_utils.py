"""Utility helpers for git repository setup and queries."""

from __future__ import annotations

import subprocess
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path


def has_git_repo(repo_root: Path) -> bool:
    """Return True if the path is inside a git work tree."""
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--is-inside-work-tree"],
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        return False

    return result.returncode == 0 and result.stdout.strip() == "true"


def list_local_branches(repo_root: Path) -> list[str]:
    """Return local branch names for a repository, if any."""
    if not has_git_repo(repo_root):
        return []

    try:
        result = subprocess.run(
            ["git", "branch", "--list", "--format", "%(refname:short)"],
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        return []

    if result.returncode != 0:
        return []

    return [line.strip() for line in result.stdout.splitlines() if line.strip()]


def get_current_branch(repo_root: Path) -> str:
    """Return the current git branch name, or empty string if unavailable."""
    if not has_git_repo(repo_root):
        return ""

    try:
        result = subprocess.run(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"],
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        return ""

    if result.returncode != 0:
        return ""

    branch = result.stdout.strip()
    return "" if branch == "HEAD" else branch


def init_git_repo(repo_root: Path, base_branch: str) -> bool:
    """Initialize a git repo with the requested base branch and initial commit.

    Creates an initial commit so that worktrees can be created from the base branch.
    Without a commit, `git worktree add -b <branch> <path> <base>` fails with
    'fatal: invalid reference: <base>'.
    """
    try:
        result = subprocess.run(
            ["git", "init", "-b", base_branch],
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        return False

    if result.returncode != 0:
        # Fallback for older git versions without -b support
        result = subprocess.run(
            ["git", "init"],
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            return False

        subprocess.run(
            ["git", "branch", "-M", base_branch],
            cwd=repo_root,
            capture_output=True,
            text=True,
            check=False,
        )

    # Create initial commit so worktrees can be created
    # Without a commit, the base branch doesn't exist as a valid reference
    gitkeep = repo_root / ".gitkeep"
    if not gitkeep.exists():
        gitkeep.write_text("")

    add_result = subprocess.run(
        ["git", "add", ".gitkeep"],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    if add_result.returncode != 0:
        return False

    commit_result = subprocess.run(
        ["git", "commit", "-m", "Initial commit (kagan)"],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    return commit_result.returncode == 0
