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
