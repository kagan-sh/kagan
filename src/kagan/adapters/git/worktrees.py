"""Git worktree management for isolated task execution."""

from __future__ import annotations

import asyncio
import re
import time
import unicodedata
from pathlib import Path
from typing import TYPE_CHECKING, Protocol

from kagan.debug_log import log
from kagan.paths import get_worktree_base_dir

if TYPE_CHECKING:
    from kagan.adapters.git.types import WorktreeInfo
    from kagan.services.types import TaskId, WorkspaceId


class WorktreeAdapter(Protocol):
    """Adapter contract for git worktree operations."""

    async def create(
        self,
        workspace_id: WorkspaceId,
        *,
        task_id: TaskId | None,
        branch_name: str,
        base_branch: str,
    ) -> WorktreeInfo:
        """Create a worktree for the given workspace."""

    async def delete(self, workspace_id: WorkspaceId, *, delete_branch: bool) -> None:
        """Delete a worktree and optionally its branch."""

    async def get_path(self, workspace_id: WorkspaceId) -> Path | None:
        """Return the worktree path, if it exists."""

    async def list_active(self) -> list[WorktreeInfo]:
        """List active worktrees managed by this adapter."""

    async def get_branch_name(self, workspace_id: WorkspaceId) -> str | None:
        """Return the branch name for a worktree."""

    async def cleanup_orphans(self, valid_workspace_ids: set[WorkspaceId]) -> list[Path]:
        """Remove worktrees not in the valid workspace list."""


class WorktreeError(Exception):
    """Raised when git worktree operations fail."""


def slugify(text: str, max_len: int = 30) -> str:
    """Convert text to URL-friendly slug.

    Examples:
        "Hello World" -> "hello-world"
        "Fix bug #123!" -> "fix-bug-123"
    """
    # Normalize unicode and convert to ASCII
    normalized = unicodedata.normalize("NFKD", text)
    ascii_text = normalized.encode("ascii", "ignore").decode("ascii")

    # Lowercase and replace non-alphanumeric with dashes
    slug = re.sub(r"[^a-z0-9]+", "-", ascii_text.lower())

    # Strip leading/trailing dashes
    slug = slug.strip("-")

    # Truncate to max_len, avoiding mid-word cuts if possible
    if len(slug) > max_len:
        slug = slug[:max_len].rstrip("-")

    return slug


class WorktreeManager:
    """Manages git worktrees for parallel task execution."""

    def __init__(self, repo_root: Path | None = None) -> None:
        """Initialize worktree manager.

        Args:
            repo_root: Root of the git repository. Defaults to cwd.
        """
        self.repo_root = repo_root or Path.cwd()
        base_dir = get_worktree_base_dir()
        self.worktrees_dir = base_dir / "worktrees"
        self._merge_worktree_dir = base_dir / "merge-worktree"
        self._merge_worktree_branch = "kagan/merge-worktree"
        self._cache_ttl_seconds = 5.0
        self._cache: dict[tuple[str, str, str], tuple[float, object]] = {}

    def _get_cached(self, key: tuple[str, str, str]) -> object | None:
        """Return cached value if within TTL."""
        now = time.monotonic()
        if key in self._cache:
            ts, value = self._cache[key]
            if now - ts <= self._cache_ttl_seconds:
                return value
            self._cache.pop(key, None)
        return None

    def _set_cached(self, key: tuple[str, str, str], value: object) -> None:
        """Store a cached value with current timestamp."""
        self._cache[key] = (time.monotonic(), value)

    async def _run_git(
        self, *args: str, check: bool = True, cwd: Path | None = None
    ) -> tuple[str, str]:
        """Run a git command and return (stdout, stderr)."""
        proc = await asyncio.create_subprocess_exec(
            "git",
            *args,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=cwd or self.repo_root,
        )
        stdout, stderr = await proc.communicate()
        stdout_str = stdout.decode().strip()
        stderr_str = stderr.decode().strip()

        if check and proc.returncode != 0:
            raise WorktreeError(stderr_str or f"git {args[0]} failed with code {proc.returncode}")

        return stdout_str, stderr_str

    def _get_worktree_path(self, task_id: str) -> Path:
        """Get the worktree path for a task."""
        return self.worktrees_dir / task_id

    def _get_merge_worktree_path(self) -> Path:
        """Get the merge worktree path."""
        return self._merge_worktree_dir

    async def _ref_exists(self, ref: str, cwd: Path) -> bool:
        """Return True if the git ref exists."""
        stdout, _ = await self._run_git(
            "rev-parse",
            "--verify",
            "--quiet",
            ref,
            cwd=cwd,
            check=False,
        )
        return bool(stdout.strip())

    async def _merge_in_progress(self, cwd: Path) -> bool:
        """Return True if a merge is in progress in the given worktree."""
        stdout, _ = await self._run_git(
            "rev-parse",
            "-q",
            "--verify",
            "MERGE_HEAD",
            cwd=cwd,
            check=False,
        )
        return bool(stdout.strip())

    async def ensure_merge_worktree(self, base_branch: str = "main") -> Path:
        """Ensure the merge worktree exists and return its path."""
        merge_path = self._get_merge_worktree_path()
        merge_path.parent.mkdir(parents=True, exist_ok=True)

        if merge_path.exists():
            return merge_path

        await self._run_git(
            "worktree",
            "add",
            "-B",
            self._merge_worktree_branch,
            str(merge_path),
            base_branch,
        )
        return merge_path

    async def get_merge_worktree_path(self, base_branch: str = "main") -> Path:
        """Return the merge worktree path, creating it if needed."""
        return await self.ensure_merge_worktree(base_branch)

    async def _reset_merge_worktree(self, base_branch: str) -> Path:
        """Reset merge worktree to the latest base branch."""
        merge_path = await self.ensure_merge_worktree(base_branch)

        await self._run_git("fetch", "origin", base_branch, cwd=merge_path, check=False)

        base_ref = base_branch
        if await self._ref_exists(f"refs/remotes/origin/{base_branch}", cwd=merge_path):
            base_ref = f"origin/{base_branch}"

        await self._run_git("checkout", self._merge_worktree_branch, cwd=merge_path)
        await self._run_git("reset", "--hard", base_ref, cwd=merge_path)
        return merge_path

    async def prepare_merge_conflicts(
        self, task_id: str, base_branch: str = "main"
    ) -> tuple[bool, str]:
        """Prepare merge worktree for manual conflict resolution."""
        branch_name = await self.get_branch_name(task_id)
        if branch_name is None:
            return False, f"Could not determine branch for task {task_id}"

        merge_path = await self.ensure_merge_worktree(base_branch)
        if await self._merge_in_progress(merge_path):
            return True, "Merge already in progress"

        try:
            await self._reset_merge_worktree(base_branch)
            await self._run_git(
                "merge",
                "--squash",
                branch_name,
                cwd=merge_path,
                check=False,
            )
            status_out, _ = await self._run_git(
                "status", "--porcelain", cwd=merge_path, check=False
            )
            if any(marker in status_out for marker in ("UU ", "AA ", "DD ")):
                return True, "Merge conflicts prepared"

            await self._run_git("merge", "--abort", cwd=merge_path, check=False)
            return False, "No conflicts detected"
        except WorktreeError as exc:
            return False, f"Prepare failed: {exc}"

    async def _fast_forward_base(self, base_branch: str) -> tuple[bool, str]:
        """Fast-forward the base branch to the merge worktree head."""
        status_out, _ = await self._run_git("status", "--porcelain", check=False)
        if status_out.strip():
            return False, (
                "Cannot update base branch: repository has uncommitted changes. "
                "Please commit or stash your changes first."
            )

        head_branch, _ = await self._run_git("rev-parse", "--abbrev-ref", "HEAD", check=False)
        if head_branch.strip() != base_branch:
            return (
                False,
                f"Cannot update base branch: checked out on '{head_branch}'. "
                f"Switch to '{base_branch}' and retry.",
            )

        try:
            # Fast-forward local branch
            await self._run_git(
                "merge",
                "--no-ff",
                self._merge_worktree_branch,
            )
        except WorktreeError as exc:
            return False, f"Fast-forward failed: {exc}"

        return True, f"Fast-forwarded {base_branch} to merge worktree"

    def _get_branch_name(self, task_id: str, title: str) -> str:
        """Generate branch name for a task."""
        slug = slugify(title)
        if slug:
            return f"kagan/{task_id}-{slug}"
        return f"kagan/{task_id}"

    async def create(self, task_id: str, title: str, base_branch: str = "main") -> Path:
        """Create a worktree for a task.

        Args:
            task_id: Unique task identifier
            title: Task title (used for branch slug)
            base_branch: Base branch to create from

        Returns:
            Path to the created worktree

        Raises:
            WorktreeError: If worktree creation fails
        """
        worktree_path = self._get_worktree_path(task_id)
        branch_name = self._get_branch_name(task_id, title)

        # Ensure worktrees directory exists
        self.worktrees_dir.mkdir(parents=True, exist_ok=True)

        # Check if worktree already exists
        if worktree_path.exists():
            raise WorktreeError(f"Worktree already exists for task {task_id}")

        # Check if branch already exists from a previous failed attempt
        # and delete it if so
        stdout, _ = await self._run_git("branch", "--list", branch_name, check=False)
        if stdout.strip():
            # Branch exists, delete it before creating worktree
            await self._run_git("branch", "-D", branch_name, check=False)

        # Create worktree with new branch
        try:
            await self._run_git(
                "worktree", "add", "-b", branch_name, str(worktree_path), base_branch
            )
        except WorktreeError as e:
            raise WorktreeError(f"Failed to create worktree for {task_id}: {e}") from e

        return worktree_path

    async def delete(self, task_id: str, delete_branch: bool = False) -> None:
        """Delete a worktree for a task. No-op if doesn't exist."""
        wt_path = self._get_worktree_path(task_id)
        if not wt_path.exists():
            return

        # Get branch name before removal if needed
        branch = None
        if delete_branch:
            stdout, _ = await self._run_git(
                "rev-parse", "--abbrev-ref", "HEAD", cwd=wt_path, check=False
            )
            branch = stdout if stdout.startswith("kagan/") else None

        # Remove worktree (force handles uncommitted changes)
        try:
            await self._run_git("worktree", "remove", str(wt_path), "--force")
        except WorktreeError:
            import shutil

            shutil.rmtree(wt_path, ignore_errors=True)
            await self._run_git("worktree", "prune", check=False)

        if branch:
            await self._run_git("branch", "-D", branch, check=False)

    async def get_path(self, task_id: str) -> Path | None:
        """Get the path to a task's worktree if it exists.

        Args:
            task_id: Unique task identifier

        Returns:
            Path to worktree if it exists, None otherwise
        """
        worktree_path = self._get_worktree_path(task_id)
        if worktree_path.exists() and worktree_path.is_dir():
            return worktree_path
        return None

    async def list_all(self) -> list[str]:
        """List all active worktree task IDs.

        Returns:
            List of task IDs that have active worktrees
        """
        if not self.worktrees_dir.exists():
            return []

        # Use git worktree list to verify actual worktrees
        try:
            stdout, _ = await self._run_git("worktree", "list", "--porcelain", check=False)
        except WorktreeError:
            return []

        # Parse worktree paths from porcelain output
        active_paths = set()
        for line in stdout.split("\n"):
            if line.startswith("worktree "):
                active_paths.add(Path(line[9:]))

        # Match against our worktree directories
        task_ids = []
        for entry in self.worktrees_dir.iterdir():
            if entry.is_dir() and entry.resolve() in active_paths:
                task_ids.append(entry.name)

        return sorted(task_ids)

    async def get_branch_name(self, task_id: str) -> str | None:
        """Get the branch name for a task's worktree.

        Args:
            task_id: Unique task identifier

        Returns:
            Branch name if worktree exists, None otherwise
        """
        wt_path = await self.get_path(task_id)
        if wt_path is None:
            return None

        try:
            stdout, _ = await self._run_git(
                "rev-parse", "--abbrev-ref", "HEAD", cwd=wt_path, check=False
            )
            return stdout if stdout else None
        except WorktreeError:
            return None

    async def get_commit_log(self, task_id: str, base_branch: str = "main") -> list[str]:
        """Get list of commit messages from the worktree branch since diverging from base.

        Args:
            task_id: Unique task identifier
            base_branch: Base branch to compare against

        Returns:
            List of commit message strings (one-line format)
        """
        cache_key = ("commit_log", task_id, base_branch)
        cached = self._get_cached(cache_key)
        if isinstance(cached, list):
            return cached

        wt_path = await self.get_path(task_id)
        if wt_path is None:
            return []

        try:
            stdout, _ = await self._run_git(
                "log", "--oneline", f"{base_branch}..HEAD", cwd=wt_path, check=False
            )
            if not stdout:
                return []
            commits = [line.strip() for line in stdout.split("\n") if line.strip()]
            self._set_cached(cache_key, commits)
            return commits
        except WorktreeError:
            return []

    async def generate_semantic_commit(self, task_id: str, title: str, commits: list[str]) -> str:
        """Generate a semantic commit message from task info and commits.

        Args:
            task_id: Unique task identifier
            title: Task title
            commits: List of commit messages to include

        Returns:
            Formatted semantic commit message
        """
        title_lower = title.lower()

        # Infer commit type from title
        if any(kw in title_lower for kw in ("fix", "bug", "issue")):
            commit_type = "fix"
        elif any(kw in title_lower for kw in ("add", "create", "implement", "new")):
            commit_type = "feat"
        elif any(kw in title_lower for kw in ("refactor", "clean", "improve")):
            commit_type = "refactor"
        elif any(kw in title_lower for kw in ("doc", "readme")):
            commit_type = "docs"
        elif "test" in title_lower:
            commit_type = "test"
        else:
            commit_type = "chore"

        # Extract scope from title if present (e.g., "Fix database connection" -> "database")
        scope = ""
        scope_match = re.match(r"^\w+\s+(\w+)", title)
        if scope_match:
            potential_scope = scope_match.group(1).lower()
            # Only use as scope if it's a reasonable component name
            if len(potential_scope) > 2 and potential_scope not in (
                "the",
                "for",
                "and",
                "with",
                "from",
                "into",
            ):
                scope = potential_scope

        # Format header
        header = f"{commit_type}({scope}): {title}" if scope else f"{commit_type}: {title}"

        # Format body with commit list
        if commits:
            # Strip commit hashes (first word) from oneline format
            body_lines = []
            for commit in commits:
                parts = commit.split(" ", 1)
                msg = parts[1] if len(parts) > 1 else commit
                body_lines.append(f"- {msg}")
            body = "\n".join(body_lines)
            return f"{header}\n\n{body}"

        return header

    async def get_diff(self, task_id: str, base_branch: str = "main") -> str:
        """Get git diff of changes compared to base branch.

        Already non-blocking via async subprocess execution.
        """
        wt_path = await self.get_path(task_id)
        if wt_path is None:
            return ""

        try:
            stdout, _ = await self._run_git(
                "diff", f"{base_branch}..HEAD", cwd=wt_path, check=False
            )
            return stdout
        except WorktreeError:
            return ""

    async def get_diff_stats(self, task_id: str, base_branch: str = "main") -> str:
        """Get diff statistics (files changed, insertions, deletions)."""
        cache_key = ("diff_stats", task_id, base_branch)
        cached = self._get_cached(cache_key)
        if isinstance(cached, str):
            return cached

        wt_path = await self.get_path(task_id)
        if wt_path is None:
            return ""

        try:
            stdout, _ = await self._run_git(
                "diff", "--stat", f"{base_branch}..HEAD", cwd=wt_path, check=False
            )
            stats = stdout.strip()
            self._set_cached(cache_key, stats)
            return stats
        except WorktreeError:
            return ""

    async def get_files_changed(self, task_id: str, base_branch: str = "main") -> list[str]:
        """Get file list changed in a task compared to base branch."""
        cache_key = ("changed_files", task_id, base_branch)
        cached = self._get_cached(cache_key)
        if isinstance(cached, list):
            return cached

        wt_path = await self.get_path(task_id)
        if wt_path is None:
            return []

        try:
            stdout, _ = await self._run_git(
                "diff", "--name-only", f"{base_branch}..HEAD", cwd=wt_path, check=False
            )
            files = [line.strip() for line in stdout.split("\n") if line.strip()]
            self._set_cached(cache_key, files)
            return files
        except WorktreeError:
            return []

    async def preflight_merge(self, task_id: str, base_branch: str = "main") -> tuple[bool, str]:
        """Check if a merge would conflict without committing changes.

        Returns:
            Tuple of (ok, message)
        """
        wt_path = await self.get_path(task_id)
        if wt_path is None:
            return False, f"Worktree not found for task {task_id}"

        branch_name = await self.get_branch_name(task_id)
        if branch_name is None:
            return False, f"Could not determine branch for task {task_id}"

        merge_started = False
        merge_path = await self.ensure_merge_worktree(base_branch)
        try:
            if await self._merge_in_progress(merge_path):
                return False, "Merge worktree has unresolved conflicts. Resolve before merging."

            await self._reset_merge_worktree(base_branch)
            merge_started = True
            await self._run_git(
                "merge",
                "--no-commit",
                "--no-ff",
                branch_name,
                cwd=merge_path,
                check=False,
            )

            status_out, _ = await self._run_git(
                "status", "--porcelain", cwd=merge_path, check=False
            )
            if any(marker in status_out for marker in ("UU ", "AA ", "DD ")):
                await self._run_git("merge", "--abort", cwd=merge_path, check=False)
                merge_started = False
                return False, "Merge conflict predicted. Please resolve before merging."

            await self._run_git("merge", "--abort", cwd=merge_path, check=False)
            merge_started = False
            return True, "Preflight clean"
        except WorktreeError as e:
            if merge_started:
                await self._run_git("merge", "--abort", cwd=merge_path, check=False)
            return False, f"Preflight failed: {e}"
        except Exception as e:
            if merge_started:
                await self._run_git("merge", "--abort", cwd=merge_path, check=False)
            return False, f"Preflight failed: {e}"

    async def rebase_onto_base(
        self, task_id: str, base_branch: str = "main"
    ) -> tuple[bool, str, list[str]]:
        """Rebase the worktree branch onto the latest base branch.

        This fetches the latest base branch and rebases the worktree branch onto it.
        Used to resolve merge conflicts by updating the branch before retry.

        Args:
            task_id: Unique task identifier
            base_branch: Base branch to rebase onto

        Returns:
            Tuple of (success, message, conflicting_files)
            - success: True if rebase completed without conflicts
            - message: Description of what happened
            - conflicting_files: List of files with conflicts (empty if success)
        """
        wt_path = await self.get_path(task_id)
        if wt_path is None:
            return False, f"Worktree not found for task {task_id}", []

        try:
            # Fetch latest changes from origin
            await self._run_git("fetch", "origin", base_branch, cwd=wt_path, check=False)

            # Get the current branch name
            branch_name = await self.get_branch_name(task_id)
            if branch_name is None:
                return False, "Could not determine branch name", []

            # Check for uncommitted changes before rebase
            status_out, _ = await self._run_git("status", "--porcelain", cwd=wt_path, check=False)
            if status_out.strip():
                return False, "Cannot rebase: worktree has uncommitted changes", []

            # Start rebase onto origin/base_branch
            stdout, stderr = await self._run_git(
                "rebase", f"origin/{base_branch}", cwd=wt_path, check=False
            )

            # Check if rebase succeeded or had conflicts
            combined_output = f"{stdout}\n{stderr}".lower()
            if "conflict" in combined_output or "could not apply" in combined_output:
                # Get list of conflicting files
                status_out, _ = await self._run_git(
                    "status", "--porcelain", cwd=wt_path, check=False
                )
                conflicting_files = []
                for line in status_out.split("\n"):
                    if line.startswith("UU ") or line.startswith("AA ") or line.startswith("DD "):
                        # Extract filename (after the status prefix)
                        filename = line[3:].strip()
                        conflicting_files.append(filename)

                # Abort the rebase to leave worktree in clean state
                await self._run_git("rebase", "--abort", cwd=wt_path, check=False)

                log.info(f"Rebase conflict for {task_id}: {conflicting_files}")
                return (
                    False,
                    f"Rebase conflict in {len(conflicting_files)} file(s)",
                    conflicting_files,
                )

            log.info(f"Successfully rebased {task_id} onto {base_branch}")
            return True, f"Successfully rebased onto {base_branch}", []

        except WorktreeError as e:
            # Abort any in-progress rebase
            await self._run_git("rebase", "--abort", cwd=wt_path, check=False)
            return False, f"Rebase failed: {e}", []
        except Exception as e:
            # Abort any in-progress rebase
            await self._run_git("rebase", "--abort", cwd=wt_path, check=False)
            return False, f"Unexpected error during rebase: {e}", []

    async def get_files_changed_on_base(self, task_id: str, base_branch: str = "main") -> list[str]:
        """Get list of files changed on the base branch since our branch diverged.

        Useful for understanding what might cause merge conflicts.

        Args:
            task_id: Unique task identifier
            base_branch: Base branch to compare against

        Returns:
            List of files changed on base branch since divergence
        """
        wt_path = await self.get_path(task_id)
        if wt_path is None:
            return []

        try:
            # Find the merge base (where we diverged from base)
            merge_base_out, _ = await self._run_git(
                "merge-base", "HEAD", f"origin/{base_branch}", cwd=wt_path, check=False
            )
            if not merge_base_out.strip():
                return []

            merge_base = merge_base_out.strip()

            # Get files changed on base branch since merge base
            diff_out, _ = await self._run_git(
                "diff", "--name-only", merge_base, f"origin/{base_branch}", cwd=wt_path, check=False
            )
            if not diff_out.strip():
                return []

            return [f.strip() for f in diff_out.split("\n") if f.strip()]
        except WorktreeError:
            return []

    async def cleanup_orphans(self, valid_task_ids: set[str]) -> list[str]:
        """Remove worktrees not associated with any known task.

        Args:
            valid_task_ids: Set of task IDs that exist in database.

        Returns:
            List of orphan task IDs that were cleaned up.
        """
        cleaned = []
        for task_id in await self.list_all():
            if task_id not in valid_task_ids:
                await self.delete(task_id, delete_branch=True)
                cleaned.append(task_id)
        return cleaned

    async def merge_to_main(
        self,
        task_id: str,
        base_branch: str = "main",
        squash: bool = True,
        allow_conflicts: bool = True,
    ) -> tuple[bool, str]:
        """Merge the worktree branch via the merge worktree.

        Args:
            task_id: Unique task identifier
            base_branch: Target branch for merge
            squash: If True, squash all commits into one with semantic message
            allow_conflicts: If True, leave conflicts in merge worktree for manual resolution

        Returns:
            Tuple of (success, message)
        """
        wt_path = await self.get_path(task_id)
        if wt_path is None:
            return False, f"Worktree not found for task {task_id}"

        branch_name = await self.get_branch_name(task_id)
        if branch_name is None:
            return False, f"Could not determine branch for task {task_id}"

        merge_path = await self.ensure_merge_worktree(base_branch)

        try:
            if await self._merge_in_progress(merge_path):
                if not allow_conflicts:
                    return False, "Merge worktree has unresolved conflicts. Resolve before merging."

                status_out, _ = await self._run_git(
                    "status", "--porcelain", cwd=merge_path, check=False
                )
                if any(marker in status_out for marker in ("UU ", "AA ", "DD ")):
                    return False, "Merge conflicts still unresolved. Finish resolution first."

                commits = await self.get_commit_log(task_id, base_branch)
                if commits:
                    title = self._format_title_from_branch(branch_name)
                    staged, _ = await self._run_git(
                        "diff", "--cached", "--name-only", cwd=merge_path, check=False
                    )
                    if staged.strip():
                        commit_msg = await self.generate_semantic_commit(task_id, title, commits)
                        await self._run_git("commit", "-m", commit_msg, cwd=merge_path)

                return await self._fast_forward_base(base_branch)

            await self._reset_merge_worktree(base_branch)

            commits = await self.get_commit_log(task_id, base_branch)
            if not commits:
                return False, f"No commits to merge for task {task_id}"

            title = self._format_title_from_branch(branch_name)

            if squash:
                await self._run_git("merge", "--squash", branch_name, cwd=merge_path, check=False)
                status_out, _ = await self._run_git(
                    "status", "--porcelain", cwd=merge_path, check=False
                )
                if any(marker in status_out for marker in ("UU ", "AA ", "DD ")):
                    if not allow_conflicts:
                        await self._run_git("merge", "--abort", cwd=merge_path, check=False)
                    return False, "Merge conflict detected. Resolve in merge worktree."

                commit_msg = await self.generate_semantic_commit(task_id, title, commits)
                await self._run_git("commit", "-m", commit_msg, cwd=merge_path)
            else:
                stdout, stderr = await self._run_git(
                    "merge",
                    branch_name,
                    "-m",
                    f"Merge branch '{branch_name}'",
                    cwd=merge_path,
                    check=False,
                )
                if "CONFLICT" in stderr or "CONFLICT" in stdout:
                    if not allow_conflicts:
                        await self._run_git("merge", "--abort", cwd=merge_path, check=False)
                    return False, "Merge conflict detected. Resolve in merge worktree."

            return await self._fast_forward_base(base_branch)
        except WorktreeError as e:
            return False, f"Merge failed: {e}"
        except Exception as e:
            return False, f"Unexpected error during merge: {e}"

    def _format_title_from_branch(self, branch_name: str) -> str:
        """Derive a readable title from a branch name."""
        title = branch_name.split("/", 1)[-1]
        if "-" in title:
            parts = title.split("-", 1)
            if len(parts) > 1:
                title = parts[1].replace("-", " ").title()
        return title
