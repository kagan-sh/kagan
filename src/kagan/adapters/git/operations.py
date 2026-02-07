"""Extended git operations for per-repo merge and diff."""

from __future__ import annotations

import asyncio
from pathlib import Path

from kagan.services.diffs import FileDiff


class GitOperationsAdapter:
    """Extended git operations for worktree-based repos."""

    async def has_uncommitted_changes(self, worktree_path: str) -> bool:
        """Check if worktree has uncommitted changes."""
        stdout, _ = await self._run_git(Path(worktree_path), ["status", "--porcelain"])
        return bool(stdout.strip())

    async def commit_all(self, worktree_path: str, message: str) -> str:
        """Stage all changes and commit."""
        if not await self.has_uncommitted_changes(worktree_path):
            stdout, _ = await self._run_git(Path(worktree_path), ["rev-parse", "HEAD"])
            return stdout.strip()

        await self._run_git(Path(worktree_path), ["add", "-A"])
        await self._run_git(Path(worktree_path), ["commit", "-m", message])
        stdout, _ = await self._run_git(Path(worktree_path), ["rev-parse", "HEAD"])
        return stdout.strip()

    async def push(self, worktree_path: str, branch: str, *, force: bool = False) -> None:
        """Push branch to origin."""
        args = ["push", "origin", branch]
        if force:
            args.insert(1, "--force-with-lease")
        await self._run_git(Path(worktree_path), args)

    async def merge_branch(self, repo_path: str, source_branch: str, target_branch: str) -> str:
        """Merge source branch into target branch and push."""
        repo_path_obj = Path(repo_path)
        await self._run_git(repo_path_obj, ["fetch", "origin", target_branch])
        await self._run_git(repo_path_obj, ["checkout", target_branch])

        stdout, stderr = await self._run_git(
            repo_path_obj,
            ["merge", "--no-ff", source_branch, "-m", f"Merge {source_branch}"],
            check=False,
        )
        if "CONFLICT" in stdout or "CONFLICT" in stderr:
            await self._run_git(repo_path_obj, ["merge", "--abort"], check=False)
            raise RuntimeError("Merge conflict detected")

        await self._run_git(repo_path_obj, ["push", "origin", target_branch])
        stdout, _ = await self._run_git(repo_path_obj, ["rev-parse", "HEAD"])
        return stdout.strip()

    async def get_file_diffs(self, worktree_path: str, target_branch: str) -> list[FileDiff]:
        """Get file-level diffs with content for a worktree."""
        diff_stats, _ = await self._run_git(
            Path(worktree_path),
            ["diff", "--numstat", f"{target_branch}..HEAD"],
        )

        files: list[FileDiff] = []
        for line in [item for item in diff_stats.split("\n") if item.strip()]:
            parts = line.split("\t")
            if len(parts) < 3:
                continue

            additions_str, deletions_str, file_path = parts[0], parts[1], parts[2]
            if " => " in file_path:
                file_path = file_path.split(" => ", maxsplit=1)[-1].strip("{}")
            additions = int(additions_str) if additions_str.isdigit() else 0
            deletions = int(deletions_str) if deletions_str.isdigit() else 0
            status = await self._get_file_status(Path(worktree_path), file_path, target_branch)

            diff_content, _ = await self._run_git(
                Path(worktree_path),
                ["diff", f"{target_branch}..HEAD", "--", file_path],
            )

            files.append(
                FileDiff(
                    path=file_path,
                    additions=additions,
                    deletions=deletions,
                    status=status,
                    diff_content=diff_content,
                )
            )

        return files

    async def _get_file_status(
        self,
        worktree_path: Path,
        file_path: str,
        target_branch: str,
    ) -> str:
        """Determine if file was added, modified, deleted, or renamed."""
        name_status, _ = await self._run_git(
            worktree_path,
            ["diff", "--name-status", f"{target_branch}..HEAD", "--", file_path],
        )

        if not name_status.strip():
            return "modified"

        status_char = name_status.strip()[0]
        status_map = {
            "A": "added",
            "M": "modified",
            "D": "deleted",
            "R": "renamed",
            "C": "copied",
        }
        return status_map.get(status_char, "modified")

    async def _run_git(self, cwd: Path, args: list[str], check: bool = True) -> tuple[str, str]:
        """Run a git command."""
        proc = await asyncio.create_subprocess_exec(
            "git",
            *args,
            cwd=cwd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await proc.communicate()

        if proc.returncode != 0 and check:
            raise RuntimeError(f"git {' '.join(args)} failed: {stderr.decode()}")

        return stdout.decode(), stderr.decode()
