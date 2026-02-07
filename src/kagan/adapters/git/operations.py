"""Extended git operations for per-repo merge and diff."""

from __future__ import annotations

import asyncio
from dataclasses import dataclass
from pathlib import Path

from kagan.services.diffs import FileDiff


@dataclass(frozen=True)
class MergeConflictMetadata:
    """Details about a merge conflict."""

    op: str
    files: list[str]


@dataclass(frozen=True)
class MergeOperationResult:
    """Result of a merge operation."""

    success: bool
    message: str
    commit_sha: str | None = None
    conflict: MergeConflictMetadata | None = None


class GitOperationsAdapter:
    """Extended git operations for worktree-based repos."""

    async def has_uncommitted_changes(self, worktree_path: str) -> bool:
        """Check if worktree has uncommitted changes to tracked files.

        Ignores untracked files (matching vibe-kanban behaviour) and
        Kagan-generated files.
        """
        from kagan.constants import KAGAN_GENERATED_PATTERNS

        stdout, _ = await self._run_git(Path(worktree_path), ["status", "--porcelain"])
        if not stdout.strip():
            return False

        for line in stdout.strip().split("\n"):
            if not line:
                continue
            # Skip untracked files — they are not uncommitted changes
            if line.startswith("?? "):
                continue
            filepath = line[3:].split(" -> ")[0]
            is_kagan_file = any(
                filepath.startswith(p.rstrip("/")) or filepath == p.rstrip("/")
                for p in KAGAN_GENERATED_PATTERNS
            )
            if not is_kagan_file:
                return True

        return False

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
        if not await self._has_remote(Path(worktree_path)):
            return
        args = ["push", "origin", branch]
        if force:
            args.insert(1, "--force-with-lease")
        await self._run_git(Path(worktree_path), args)

    async def merge_squash(
        self,
        repo_path: str,
        source_branch: str,
        target_branch: str,
        *,
        commit_message: str | None = None,
    ) -> MergeOperationResult:
        """Squash merge source branch into target branch and push."""
        repo_path_obj = Path(repo_path)
        has_origin = await self._has_remote(repo_path_obj)
        if has_origin:
            await self._run_git(repo_path_obj, ["fetch", "origin", target_branch])

        base_ref = await self._resolve_base_ref(repo_path_obj, target_branch)
        if await self.is_base_ahead(repo_path, base_ref, source_branch):
            return MergeOperationResult(
                success=False,
                message=f"Base branch {base_ref} is ahead of {source_branch}; rebase required",
            )

        await self._run_git(repo_path_obj, ["checkout", target_branch])

        returncode, stdout, stderr = await self._run_git_result(
            repo_path_obj,
            ["merge", "--squash", source_branch],
        )

        conflict_files = await self._collect_conflict_files(repo_path_obj)
        if conflict_files:
            await self._abort_merge(repo_path_obj)
            return MergeOperationResult(
                success=False,
                message="Merge conflict detected",
                conflict=MergeConflictMetadata(op="merge-squash", files=conflict_files),
            )

        if returncode != 0:
            await self._abort_merge(repo_path_obj)
            error_message = stderr.strip() or stdout.strip() or "Squash merge failed"
            return MergeOperationResult(
                success=False,
                message=error_message,
            )

        status_out, _ = await self._run_git(repo_path_obj, ["status", "--porcelain"])
        if not status_out.strip():
            stdout_head, _ = await self._run_git(repo_path_obj, ["rev-parse", "HEAD"])
            return MergeOperationResult(
                success=True,
                message="No changes to merge",
                commit_sha=stdout_head.strip(),
            )

        await self._run_git(
            repo_path_obj,
            ["commit", "-m", commit_message or f"Squash merge {source_branch}"],
        )

        if has_origin:
            await self._run_git(repo_path_obj, ["push", "origin", target_branch])

        stdout_head, _ = await self._run_git(repo_path_obj, ["rev-parse", "HEAD"])
        squash_sha = stdout_head.strip()

        # Update task branch ref to point to the squash commit so follow-up
        # work can continue from the merged state without conflicts.
        await self._run_git(
            repo_path_obj,
            ["update-ref", f"refs/heads/{source_branch}", squash_sha],
            check=False,
        )

        return MergeOperationResult(
            success=True,
            message=f"Squash merged to {target_branch}",
            commit_sha=squash_sha,
        )

    async def merge_branch(self, repo_path: str, source_branch: str, target_branch: str) -> str:
        """Merge source branch into target branch and push."""
        repo_path_obj = Path(repo_path)
        has_origin = await self._has_remote(repo_path_obj)
        if has_origin:
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

        if has_origin:
            await self._run_git(repo_path_obj, ["push", "origin", target_branch])
        stdout, _ = await self._run_git(repo_path_obj, ["rev-parse", "HEAD"])
        return stdout.strip()

    async def is_base_ahead(self, repo_path: str, base_ref: str, head_ref: str) -> bool:
        """Return True if base_ref is ahead of head_ref."""
        stdout, _ = await self._run_git(
            Path(repo_path),
            ["rev-list", "--left-right", "--count", f"{base_ref}...{head_ref}"],
        )
        left_count_str = stdout.strip().split()[0] if stdout.strip() else "0"
        return int(left_count_str) > 0

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

    async def _has_remote(self, repo_path: Path, name: str = "origin") -> bool:
        stdout, _ = await self._run_git(repo_path, ["remote"], check=False)
        return name in {item.strip() for item in stdout.splitlines() if item.strip()}

    async def _resolve_base_ref(self, repo_path: Path, base_branch: str) -> str:
        if await self._ref_exists(repo_path, f"refs/remotes/origin/{base_branch}"):
            return f"origin/{base_branch}"
        return base_branch

    async def _ref_exists(self, repo_path: Path, ref: str) -> bool:
        stdout, _ = await self._run_git(
            repo_path,
            ["rev-parse", "--verify", "--quiet", ref],
            check=False,
        )
        return bool(stdout.strip())

    async def _collect_conflict_files(self, repo_path: Path) -> list[str]:
        stdout, _ = await self._run_git(
            repo_path,
            ["diff", "--name-only", "--diff-filter=U"],
            check=False,
        )
        files = [line.strip() for line in stdout.splitlines() if line.strip()]
        if files:
            return files

        status_out, _ = await self._run_git(repo_path, ["status", "--porcelain"], check=False)
        files = []
        for line in status_out.splitlines():
            if line.startswith(("UU ", "AA ", "DD ")):
                files.append(line[3:].strip())
        return files

    async def _abort_merge(self, repo_path: Path) -> None:
        await self._run_git(repo_path, ["merge", "--abort"], check=False)
        await self._run_git(repo_path, ["reset", "--hard"], check=False)

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

    async def _run_git_result(self, cwd: Path, args: list[str]) -> tuple[int, str, str]:
        proc = await asyncio.create_subprocess_exec(
            "git",
            *args,
            cwd=cwd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await proc.communicate()
        returncode = proc.returncode if proc.returncode is not None else 1
        return returncode, stdout.decode(), stderr.decode()
