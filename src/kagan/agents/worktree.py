"""Git worktree management for isolated ticket execution."""

from __future__ import annotations

import asyncio
import re
import unicodedata
from pathlib import Path


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
    """Manages git worktrees for parallel ticket execution."""

    def __init__(self, repo_root: Path | None = None) -> None:
        """Initialize worktree manager.

        Args:
            repo_root: Root of the git repository. Defaults to cwd.
        """
        self.repo_root = repo_root or Path.cwd()
        self.worktrees_dir = self.repo_root / ".kagan" / "worktrees"

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

    def _get_worktree_path(self, ticket_id: str) -> Path:
        """Get the worktree path for a ticket."""
        return self.worktrees_dir / ticket_id

    def _get_branch_name(self, ticket_id: str, title: str) -> str:
        """Generate branch name for a ticket."""
        slug = slugify(title)
        if slug:
            return f"kagan/{ticket_id}-{slug}"
        return f"kagan/{ticket_id}"

    async def create(self, ticket_id: str, title: str, base_branch: str = "main") -> Path:
        """Create a worktree for a ticket.

        Args:
            ticket_id: Unique ticket identifier
            title: Ticket title (used for branch slug)
            base_branch: Base branch to create from

        Returns:
            Path to the created worktree

        Raises:
            WorktreeError: If worktree creation fails
        """
        worktree_path = self._get_worktree_path(ticket_id)
        branch_name = self._get_branch_name(ticket_id, title)

        # Ensure worktrees directory exists
        self.worktrees_dir.mkdir(parents=True, exist_ok=True)

        # Check if worktree already exists
        if worktree_path.exists():
            raise WorktreeError(f"Worktree already exists for ticket {ticket_id}")

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
            raise WorktreeError(f"Failed to create worktree for {ticket_id}: {e}") from e

        return worktree_path

    async def delete(self, ticket_id: str, delete_branch: bool = False) -> None:
        """Delete a worktree for a ticket. No-op if doesn't exist."""
        wt_path = self._get_worktree_path(ticket_id)
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

    async def get_path(self, ticket_id: str) -> Path | None:
        """Get the path to a ticket's worktree if it exists.

        Args:
            ticket_id: Unique ticket identifier

        Returns:
            Path to worktree if it exists, None otherwise
        """
        worktree_path = self._get_worktree_path(ticket_id)
        if worktree_path.exists() and worktree_path.is_dir():
            return worktree_path
        return None

    async def list_all(self) -> list[str]:
        """List all active worktree ticket IDs.

        Returns:
            List of ticket IDs that have active worktrees
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
        ticket_ids = []
        for entry in self.worktrees_dir.iterdir():
            if entry.is_dir() and entry.resolve() in active_paths:
                ticket_ids.append(entry.name)

        return sorted(ticket_ids)

    async def get_branch_name(self, ticket_id: str) -> str | None:
        """Get the branch name for a ticket's worktree.

        Args:
            ticket_id: Unique ticket identifier

        Returns:
            Branch name if worktree exists, None otherwise
        """
        wt_path = await self.get_path(ticket_id)
        if wt_path is None:
            return None

        try:
            stdout, _ = await self._run_git(
                "rev-parse", "--abbrev-ref", "HEAD", cwd=wt_path, check=False
            )
            return stdout if stdout else None
        except WorktreeError:
            return None

    async def get_commit_log(self, ticket_id: str, base_branch: str = "main") -> list[str]:
        """Get list of commit messages from the worktree branch since diverging from base.

        Args:
            ticket_id: Unique ticket identifier
            base_branch: Base branch to compare against

        Returns:
            List of commit message strings (one-line format)
        """
        wt_path = await self.get_path(ticket_id)
        if wt_path is None:
            return []

        try:
            stdout, _ = await self._run_git(
                "log", "--oneline", f"{base_branch}..HEAD", cwd=wt_path, check=False
            )
            if not stdout:
                return []
            return [line.strip() for line in stdout.split("\n") if line.strip()]
        except WorktreeError:
            return []

    async def generate_semantic_commit(self, ticket_id: str, title: str, commits: list[str]) -> str:
        """Generate a semantic commit message from ticket info and commits.

        Args:
            ticket_id: Unique ticket identifier
            title: Ticket title
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

    async def get_diff(self, ticket_id: str, base_branch: str = "main") -> str:
        """Get diff output for a ticket's worktree."""
        wt_path = await self.get_path(ticket_id)
        if wt_path is None:
            return ""

        try:
            stdout, _ = await self._run_git(
                "diff", f"{base_branch}..HEAD", cwd=wt_path, check=False
            )
            return stdout
        except WorktreeError:
            return ""

    async def get_diff_stats(self, ticket_id: str, base_branch: str = "main") -> str:
        """Get diff statistics (files changed, insertions, deletions)."""
        wt_path = await self.get_path(ticket_id)
        if wt_path is None:
            return ""

        try:
            stdout, _ = await self._run_git(
                "diff", "--stat", f"{base_branch}..HEAD", cwd=wt_path, check=False
            )
            return stdout.strip()
        except WorktreeError:
            return ""

    async def merge_to_main(
        self, ticket_id: str, base_branch: str = "main", squash: bool = True
    ) -> tuple[bool, str]:
        """Merge the worktree branch back to the base branch.

        Args:
            ticket_id: Unique ticket identifier
            base_branch: Target branch for merge
            squash: If True, squash all commits into one with semantic message

        Returns:
            Tuple of (success, message)
        """
        # Get worktree path and branch
        wt_path = await self.get_path(ticket_id)
        if wt_path is None:
            return False, f"Worktree not found for ticket {ticket_id}"

        branch_name = await self.get_branch_name(ticket_id)
        if branch_name is None:
            return False, f"Could not determine branch for ticket {ticket_id}"

        try:
            # Get commit log for semantic message
            commits = await self.get_commit_log(ticket_id, base_branch)
            if not commits:
                return False, f"No commits to merge for ticket {ticket_id}"

            # Read ticket title from branch name slug (fallback)
            title = branch_name.split("/", 1)[-1]  # Remove kagan/ prefix
            if "-" in title:
                # Extract title part after ticket ID
                parts = title.split("-", 1)
                if len(parts) > 1:
                    title = parts[1].replace("-", " ").title()

            # Checkout base branch in main repo
            await self._run_git("checkout", base_branch)

            # Merge the worktree branch
            if squash:
                await self._run_git("merge", "--squash", branch_name, check=False)
                # Check for merge conflicts
                status_out, _ = await self._run_git("status", "--porcelain", check=False)
                if "UU " in status_out or "AA " in status_out or "DD " in status_out:
                    # Abort the merge
                    await self._run_git("merge", "--abort", check=False)
                    return False, "Merge conflict detected. Please resolve manually."

                # Generate semantic commit message
                commit_msg = await self.generate_semantic_commit(ticket_id, title, commits)

                # Commit the squashed changes
                await self._run_git("commit", "-m", commit_msg)
            else:
                # Regular merge
                stdout, stderr = await self._run_git(
                    "merge", branch_name, "-m", f"Merge branch '{branch_name}'", check=False
                )
                if "CONFLICT" in stderr or "CONFLICT" in stdout:
                    await self._run_git("merge", "--abort", check=False)
                    return False, "Merge conflict detected. Please resolve manually."

            return True, f"Successfully merged {branch_name} to {base_branch}"

        except WorktreeError as e:
            return False, f"Merge failed: {e}"
        except Exception as e:
            return False, f"Unexpected error during merge: {e}"
