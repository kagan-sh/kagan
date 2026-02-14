"""Git worktree management for isolated task execution."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Protocol

from kagan.core.adapters.git.operations import GitAdapterBase, GitCommandRunner
from kagan.core.config import WORKTREE_BASE_REF_STRATEGY_VALUES, WorktreeBaseRefStrategyLiteral


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

    async def prune_worktrees(self, repo_path: str) -> int: ...

    async def list_kagan_branches(self, repo_path: str) -> list[str]: ...

    async def delete_branch(
        self, repo_path: str, branch_name: str, *, force: bool = False
    ) -> bool: ...


class GitWorktreeAdapter(GitAdapterBase):
    """Adapter for git worktree operations across multiple repositories."""

    def __init__(
        self,
        runner: GitCommandRunner | None = None,
        *,
        base_ref_strategy: WorktreeBaseRefStrategyLiteral = "remote",
    ) -> None:
        super().__init__(runner)
        if base_ref_strategy not in WORKTREE_BASE_REF_STRATEGY_VALUES:
            options = ", ".join(sorted(WORKTREE_BASE_REF_STRATEGY_VALUES))
            raise ValueError(f"base_ref_strategy must be one of: {options}")
        self._base_ref_strategy = base_ref_strategy

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
        start_point = await self._resolve_base_ref_with_strategy(
            repo_path_obj,
            base_branch,
            refresh_remote=True,
        )

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
        return await self._resolve_base_ref_with_strategy(cwd, base_branch, refresh_remote=False)

    async def _resolve_base_ref_with_strategy(
        self,
        cwd: Path,
        base_branch: str,
        *,
        refresh_remote: bool,
    ) -> str:
        """Resolve the base ref according to configured strategy."""
        if self._base_ref_strategy == "local":
            if await self._has_local_branch(cwd, base_branch):
                return base_branch
            if await self._has_remote_branch(cwd, base_branch):
                return f"origin/{base_branch}"
            return base_branch

        if refresh_remote and await self._has_remote(cwd, "origin"):
            await self._run_git(cwd, ["fetch", "origin", base_branch], check=False)

        has_local = await self._has_local_branch(cwd, base_branch)
        has_remote = await self._has_remote_branch(cwd, base_branch)

        if self._base_ref_strategy == "remote":
            if has_remote:
                return f"origin/{base_branch}"
            if has_local:
                return base_branch
            return base_branch

        if has_local and has_remote:
            if await self._is_local_ahead_of_origin(cwd, base_branch):
                return base_branch
            return f"origin/{base_branch}"
        if has_remote:
            return f"origin/{base_branch}"
        if has_local:
            return base_branch
        return base_branch

    async def _has_local_branch(self, cwd: Path, branch: str) -> bool:
        return await self._ref_exists(cwd, f"refs/heads/{branch}")

    async def _has_remote_branch(self, cwd: Path, branch: str) -> bool:
        return await self._ref_exists(cwd, f"refs/remotes/origin/{branch}")

    async def _ref_exists(self, cwd: Path, ref: str) -> bool:
        stdout, _ = await self._run_git(
            cwd,
            ["rev-parse", "--verify", "--quiet", ref],
            check=False,
        )
        return bool(stdout.strip())

    async def _is_local_ahead_of_origin(self, cwd: Path, branch: str) -> bool:
        stdout, _ = await self._run_git(
            cwd,
            [
                "rev-list",
                "--count",
                f"refs/remotes/origin/{branch}..refs/heads/{branch}",
            ],
            check=False,
        )
        try:
            return int(stdout.strip()) > 0
        except ValueError:
            return False

    async def run_git(self, *args: str, cwd: Path, check: bool = True) -> tuple[str, str]:
        """Run an arbitrary git command, returning (stdout, stderr)."""
        stdout, stderr = await self._run_git(cwd, list(args), check=check)
        return stdout.strip(), stderr.strip()

    def _extract_number(self, text: str, word: str) -> int:
        """Extract number before a word in text."""
        match = re.search(rf"(\d+)\s+{word}", text)
        return int(match.group(1)) if match else 0

    async def prune_worktrees(self, repo_path: str) -> int:
        """Prune stale worktree references from a repository.

        Runs `git worktree prune` to clean up worktree administrative files
        for worktrees that no longer exist on disk.

        Returns the number of worktrees pruned (estimated from output).
        """
        repo_path_obj = Path(repo_path)
        if not repo_path_obj.exists():
            return 0

        stdout, _ = await self._run_git(
            repo_path_obj,
            ["worktree", "prune", "--verbose"],
            check=False,
        )
        pruned_lines = [line for line in stdout.split("\n") if line.strip().startswith("Removing")]
        return len(pruned_lines)

    async def list_kagan_branches(self, repo_path: str) -> list[str]:
        """List all local branches matching the kagan/* pattern.

        Returns branch names without the refs/heads/ prefix.
        """
        repo_path_obj = Path(repo_path)
        if not repo_path_obj.exists():
            return []

        stdout, _ = await self._run_git(
            repo_path_obj,
            ["for-each-ref", "--format=%(refname:short)", "refs/heads/kagan/*"],
            check=False,
        )
        return [line.strip() for line in stdout.split("\n") if line.strip()]

    async def delete_branch(
        self,
        repo_path: str,
        branch_name: str,
        *,
        force: bool = False,
    ) -> bool:
        """Delete a local branch.

        Args:
            repo_path: Path to the repository.
            branch_name: Name of the branch to delete.
            force: If True, use -D (force delete); otherwise use -d (safe delete).

        Returns:
            True if the branch was deleted, False if it failed or didn't exist.
        """
        repo_path_obj = Path(repo_path)
        if not repo_path_obj.exists():
            return False

        delete_flag = "-D" if force else "-d"
        returncode, _, _ = await self._run_git_result(
            repo_path_obj,
            ["branch", delete_flag, branch_name],
        )
        return returncode == 0

    async def get_worktree_for_branch(self, repo_path: str, branch_name: str) -> str | None:
        """Get the worktree path for a branch, if any.

        Returns the worktree path if the branch is checked out in a worktree,
        or None if not.
        """
        repo_path_obj = Path(repo_path)
        if not repo_path_obj.exists():
            return None

        stdout, _ = await self._run_git(
            repo_path_obj,
            ["worktree", "list", "--porcelain"],
            check=False,
        )

        current_worktree: str | None = None
        for line in stdout.split("\n"):
            if line.startswith("worktree "):
                current_worktree = line[9:].strip()
            elif line.startswith("branch "):
                branch_ref = line[7:].strip()
                if branch_ref == f"refs/heads/{branch_name}":
                    return current_worktree

        return None
