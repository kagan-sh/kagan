"""Shared git command runner, base adapter, and extended git operations."""

from __future__ import annotations

from dataclasses import dataclass
from fnmatch import fnmatch
from pathlib import Path
from typing import TYPE_CHECKING, Protocol

from kagan.core.adapters.process import (
    ProcessExecutionError,
    ProcessRetryPolicy,
    run_exec_capture,
    run_exec_checked,
)
from kagan.core.constants import KAGAN_GENERATED_PATTERNS
from kagan.core.services.diffs import FileDiff

if TYPE_CHECKING:
    from collections.abc import Sequence

# ---------------------------------------------------------------------------
# Shared types and command runner
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class GitCommandResult:
    """Result of a git command invocation."""

    returncode: int
    stdout: str
    stderr: str


class GitCommandRunner:
    """Run git commands in subprocesses."""

    async def run(self, cwd: Path, args: Sequence[str], *, check: bool = True) -> GitCommandResult:
        try:
            if check:
                result = await run_exec_checked(
                    "git",
                    *args,
                    cwd=cwd,
                    retry_policy=ProcessRetryPolicy(max_attempts=2, delay_seconds=0.1),
                )
            else:
                result = await run_exec_capture("git", *args, cwd=cwd)
        except FileNotFoundError as exc:  # pragma: no cover - environment dependent
            raise RuntimeError("git executable not found") from exc
        except ProcessExecutionError as exc:
            raise RuntimeError(str(exc)) from exc

        returncode = result.returncode
        stdout = result.stdout_text()
        stderr = result.stderr_text()

        if check and returncode != 0:
            cmd = " ".join(args)
            err = stderr.strip() or stdout.strip() or "unknown git error"
            raise RuntimeError(f"git {cmd} failed (rc={returncode}): {err}")

        return GitCommandResult(returncode=returncode, stdout=stdout, stderr=stderr)


class GitAdapterBase:
    """Base helper for git adapters with shared execution and status checks."""

    def __init__(self, runner: GitCommandRunner | None = None) -> None:
        self._runner = runner or GitCommandRunner()

    async def _run_git(
        self,
        cwd: Path,
        args: Sequence[str],
        *,
        check: bool = True,
    ) -> tuple[str, str]:
        result = await self._runner.run(cwd, args, check=check)
        return result.stdout, result.stderr

    async def _run_git_result(self, cwd: Path, args: Sequence[str]) -> tuple[int, str, str]:
        result = await self._runner.run(cwd, args, check=False)
        return result.returncode, result.stdout, result.stderr

    async def _has_uncommitted_changes(self, cwd: Path) -> bool:
        stdout, _ = await self._run_git(cwd, ["status", "--porcelain"], check=False)
        return has_tracked_uncommitted_changes(stdout)


def has_tracked_uncommitted_changes(status_output: str) -> bool:
    """Check `git status --porcelain` output for relevant uncommitted changes.

    Canonical behavior:
    - Ignore untracked files.
    - Ignore Kagan-generated/config files.
    - Treat all other tracked changes as uncommitted.
    """
    if not status_output.strip():
        return False

    for raw_line in status_output.splitlines():
        line = raw_line.rstrip()
        if not line:
            continue

        status = line[:2]
        if status == "??":
            continue

        path_segment = line[3:] if len(line) > 3 else ""
        for path in _extract_status_paths(path_segment):
            if path and not _is_kagan_generated_path(path):
                return True

    return False


def _extract_status_paths(path_segment: str) -> list[str]:
    raw_paths = path_segment.split(" -> ") if " -> " in path_segment else [path_segment]
    return [_normalize_status_path(path) for path in raw_paths]


def _normalize_status_path(path: str) -> str:
    normalized = path.strip()
    if len(normalized) >= 2 and normalized[0] == normalized[-1] == '"':
        return normalized[1:-1]
    return normalized


def _is_kagan_generated_path(path: str) -> bool:
    normalized = path.strip().lstrip("./")

    for pattern in KAGAN_GENERATED_PATTERNS:
        if pattern.endswith("/"):
            prefix = pattern.rstrip("/")
            if normalized == prefix or normalized.startswith(f"{prefix}/"):
                return True
            continue

        if fnmatch(normalized, pattern):
            return True

    return False


# ---------------------------------------------------------------------------
# Extended git operations
# ---------------------------------------------------------------------------


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


class GitOperationsProtocol(Protocol):
    """Protocol boundary for higher-level git operations used by services."""

    async def has_uncommitted_changes(self, worktree_path: str) -> bool: ...

    async def commit_all(self, worktree_path: str, message: str) -> str: ...

    async def push(self, worktree_path: str, branch: str, *, force: bool = False) -> None: ...

    async def merge_squash(
        self,
        repo_path: str,
        source_branch: str,
        target_branch: str,
        *,
        commit_message: str | None = None,
    ) -> MergeOperationResult: ...

    async def merge_branch(self, repo_path: str, source_branch: str, target_branch: str) -> str: ...

    async def is_base_ahead(self, repo_path: str, base_ref: str, head_ref: str) -> bool: ...

    async def get_file_diffs(self, worktree_path: str, target_branch: str) -> list[FileDiff]: ...


class GitOperationsAdapter(GitAdapterBase):
    """Extended git operations for worktree-based repos."""

    async def has_uncommitted_changes(self, worktree_path: str) -> bool:
        """Check if worktree has uncommitted tracked changes."""
        return await self._has_uncommitted_changes(Path(worktree_path))

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
