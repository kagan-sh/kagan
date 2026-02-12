"""Git worktree management for isolated task execution."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Protocol

from kagan.core.adapters.git.operations import GitAdapterBase


class GitWorktreeProtocol(Protocol):
    """Protocol boundary for git worktree and repo-diff operations."""

    async def create_worktree(
        self,
        repo_path: str,
        worktree_path: str,
        branch_name: str,
        base_branch: str = "main",
    ) -> None: ...

    async def delete_worktree(self, worktree_path: str) -> None: ...

    async def has_uncommitted_changes(self, worktree_path: str) -> bool: ...

    async def get_diff(self, worktree_path: str, target_branch: str) -> str: ...

    async def get_diff_stats(self, worktree_path: str, target_branch: str) -> dict: ...

    async def get_commit_log(self, worktree_path: str, base_branch: str) -> list[str]: ...

    async def get_files_changed(self, worktree_path: str, base_branch: str) -> list[str]: ...

    async def run_git(self, *args: str, cwd: Path, check: bool = True) -> tuple[str, str]: ...


class GitWorktreeAdapter(GitAdapterBase):
    """Adapter for git worktree operations across multiple repositories."""

    async def create_worktree(
        self,
        repo_path: str,
        worktree_path: str,
        branch_name: str,
        base_branch: str = "main",
    ) -> None:
        """Create a new git worktree."""
        repo_path_obj = Path(repo_path)
        worktree_path_obj = Path(worktree_path)

        try:
            await self._run_git(repo_path_obj, ["fetch", "origin", base_branch])
            start_point = f"origin/{base_branch}"
        except RuntimeError:
            start_point = base_branch

        await self._run_git(
            repo_path_obj,
            [
                "worktree",
                "add",
                "-b",
                branch_name,
                str(worktree_path_obj),
                start_point,
            ],
        )

    async def _has_remote(self, repo_path: Path, remote_name: str) -> bool:
        """Check if a remote exists in the repository."""
        stdout, _ = await self._run_git(repo_path, ["remote"], check=False)
        remotes = [r.strip() for r in stdout.split("\n") if r.strip()]
        return remote_name in remotes

    async def delete_worktree(self, worktree_path: str) -> None:
        """Delete a git worktree."""
        worktree_path_obj = Path(worktree_path)

        if not worktree_path_obj.exists():
            return

        git_file = worktree_path_obj / ".git"
        if not git_file.exists():
            return

        content = git_file.read_text().strip()
        if not content.startswith("gitdir:"):
            return

        git_dir = content.split(":", 1)[1].strip()
        main_repo = Path(git_dir).parent.parent.parent

        await self._run_git(
            main_repo,
            ["worktree", "remove", str(worktree_path_obj), "--force"],
        )

    async def has_uncommitted_changes(self, worktree_path: str) -> bool:
        """Check if worktree has uncommitted changes."""
        worktree_path_obj = Path(worktree_path)
        if not worktree_path_obj.exists():
            return False
        return await self._has_uncommitted_changes(worktree_path_obj)

    async def get_diff(
        self,
        worktree_path: str,
        target_branch: str,
    ) -> str:
        """Get diff between worktree and target branch."""
        worktree_path_obj = Path(worktree_path)
        if not worktree_path_obj.exists():
            return ""
        base_ref = await self._resolve_base_ref(worktree_path_obj, target_branch)
        stdout, _ = await self._run_git(
            worktree_path_obj,
            ["diff", f"{base_ref}..HEAD"],
        )
        return stdout

    async def get_diff_stats(
        self,
        worktree_path: str,
        target_branch: str,
    ) -> dict:
        """Get diff statistics."""
        worktree_path_obj = Path(worktree_path)
        if not worktree_path_obj.exists():
            return {"files": 0, "insertions": 0, "deletions": 0}
        base_ref = await self._resolve_base_ref(worktree_path_obj, target_branch)
        stat_output, _ = await self._run_git(
            worktree_path_obj,
            ["diff", "--stat", f"{base_ref}..HEAD"],
        )

        lines = stat_output.strip().split("\n")
        if not lines:
            return {"files": 0, "insertions": 0, "deletions": 0}

        summary = lines[-1] if lines else ""

        return {
            "files": self._extract_number(summary, "file"),
            "insertions": self._extract_number(summary, "insertion"),
            "deletions": self._extract_number(summary, "deletion"),
            "stat_lines": lines[:-1],
        }

    async def get_commit_log(self, worktree_path: str, base_branch: str) -> list[str]:
        """Get commit log for the worktree."""
        worktree_path_obj = Path(worktree_path)
        if not worktree_path_obj.exists():
            return []
        base_ref = await self._resolve_base_ref(worktree_path_obj, base_branch)
        stdout, _ = await self._run_git(
            worktree_path_obj,
            ["log", "--oneline", f"{base_ref}..HEAD"],
        )
        return [line.strip() for line in stdout.split("\n") if line.strip()]

    async def get_files_changed(self, worktree_path: str, base_branch: str) -> list[str]:
        """Get file list changed in a worktree compared to base branch."""
        worktree_path_obj = Path(worktree_path)
        if not worktree_path_obj.exists():
            return []
        base_ref = await self._resolve_base_ref(worktree_path_obj, base_branch)
        stdout, _ = await self._run_git(
            worktree_path_obj,
            ["diff", "--name-only", f"{base_ref}..HEAD"],
        )
        return [line.strip() for line in stdout.split("\n") if line.strip()]

    async def _resolve_base_ref(self, cwd: Path, base_branch: str) -> str:
        """Prefer origin/<base_branch> when it exists."""
        stdout, _ = await self._run_git(
            cwd,
            ["rev-parse", "--verify", "--quiet", f"refs/remotes/origin/{base_branch}"],
            check=False,
        )
        return f"origin/{base_branch}" if stdout.strip() else base_branch

    async def run_git(self, *args: str, cwd: Path, check: bool = True) -> tuple[str, str]:
        """Run an arbitrary git command, returning (stdout, stderr)."""
        stdout, stderr = await self._run_git(cwd, list(args), check=check)
        return stdout.strip(), stderr.strip()

    def _extract_number(self, text: str, word: str) -> int:
        """Extract number before a word in text."""
        match = re.search(rf"(\d+)\s+{word}", text)
        return int(match.group(1)) if match else 0
