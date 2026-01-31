"""Utility helpers for git repository setup and queries.

All functions are async to avoid blocking the event loop during subprocess calls.
"""

from __future__ import annotations

import asyncio
import re
from dataclasses import dataclass
from pathlib import Path  # noqa: TC003 - Used at runtime in _ensure_gitignored

# Minimum git version required for worktree support
# git worktree was introduced in Git 2.5 (July 2015)
MIN_GIT_VERSION = (2, 5, 0)


@dataclass
class GitVersion:
    """Parsed git version information."""

    major: int
    minor: int
    patch: int
    raw: str

    def __ge__(self, other: tuple[int, int, int]) -> bool:
        return (self.major, self.minor, self.patch) >= other

    def __lt__(self, other: tuple[int, int, int]) -> bool:
        return (self.major, self.minor, self.patch) < other

    def __str__(self) -> str:
        return f"{self.major}.{self.minor}.{self.patch}"


@dataclass
class GitError:
    """Describes a git operation failure."""

    error_type: str  # "version_low", "user_not_configured", "init_failed", "commit_failed"
    message: str
    details: str | None = None


@dataclass
class GitInitResult:
    """Result of git repository initialization."""

    success: bool
    error: GitError | None = None
    gitignore_created: bool = False
    gitignore_updated: bool = False
    committed: bool = False


async def get_git_version() -> GitVersion | None:
    """Get the installed git version.

    Returns None if git is not installed or version cannot be determined.
    """
    try:
        proc = await asyncio.create_subprocess_exec(
            "git",
            "--version",
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _ = await proc.communicate()
        if proc.returncode != 0:
            return None

        # Parse "git version X.Y.Z" or "git version X.Y.Z.windows.N" etc.
        raw = stdout.decode().strip()
        match = re.search(r"(\d+)\.(\d+)\.(\d+)", raw)
        if not match:
            return None

        return GitVersion(
            major=int(match.group(1)),
            minor=int(match.group(2)),
            patch=int(match.group(3)),
            raw=raw,
        )
    except FileNotFoundError:
        return None


async def check_git_user_configured() -> tuple[bool, str | None]:
    """Check if git user.name and user.email are configured.

    Returns (True, None) if configured, (False, error_message) otherwise.
    """
    try:
        # Check user.name
        proc_name = await asyncio.create_subprocess_exec(
            "git",
            "config",
            "--get",
            "user.name",
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout_name, _ = await proc_name.communicate()

        # Check user.email
        proc_email = await asyncio.create_subprocess_exec(
            "git",
            "config",
            "--get",
            "user.email",
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout_email, _ = await proc_email.communicate()

        name_set = proc_name.returncode == 0 and stdout_name.decode().strip()
        email_set = proc_email.returncode == 0 and stdout_email.decode().strip()

        if not name_set and not email_set:
            return False, "Git user.name and user.email are not configured"
        if not name_set:
            return False, "Git user.name is not configured"
        if not email_set:
            return False, "Git user.email is not configured"

        return True, None
    except FileNotFoundError:
        return False, "Git is not installed"


async def has_git_repo(repo_root: Path) -> bool:
    """Return True if the path is inside a git work tree."""
    try:
        proc = await asyncio.create_subprocess_exec(
            "git",
            "rev-parse",
            "--is-inside-work-tree",
            cwd=repo_root,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _ = await proc.communicate()
        return proc.returncode == 0 and stdout.decode().strip() == "true"
    except FileNotFoundError:
        return False


async def list_local_branches(repo_root: Path) -> list[str]:
    """Return local branch names for a repository, if any."""
    if not await has_git_repo(repo_root):
        return []
    try:
        proc = await asyncio.create_subprocess_exec(
            "git",
            "branch",
            "--list",
            "--format",
            "%(refname:short)",
            cwd=repo_root,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _ = await proc.communicate()
        if proc.returncode != 0:
            return []
        return [line.strip() for line in stdout.decode().splitlines() if line.strip()]
    except FileNotFoundError:
        return []


async def get_current_branch(repo_root: Path) -> str:
    """Return the current git branch name, or empty string if unavailable."""
    if not await has_git_repo(repo_root):
        return ""
    try:
        proc = await asyncio.create_subprocess_exec(
            "git",
            "rev-parse",
            "--abbrev-ref",
            "HEAD",
            cwd=repo_root,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _ = await proc.communicate()
        if proc.returncode != 0:
            return ""
        branch = stdout.decode().strip()
        return "" if branch == "HEAD" else branch
    except FileNotFoundError:
        return ""


async def has_commits(repo_root: Path) -> bool:
    """Return True if the git repo has at least one commit."""
    try:
        proc = await asyncio.create_subprocess_exec(
            "git",
            "rev-parse",
            "HEAD",
            cwd=repo_root,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        await proc.communicate()
        return proc.returncode == 0
    except FileNotFoundError:
        return False


def _ensure_gitignored(repo_root: Path) -> tuple[bool, bool]:
    """Add .kagan/ to .gitignore if not already present.

    Returns (created, updated) tuple:
    - created: True if .gitignore was created from scratch
    - updated: True if .gitignore was modified (appended to)
    """
    gitignore = repo_root / ".gitignore"
    kagan_entry = ".kagan/"

    if gitignore.exists():
        content = gitignore.read_text()
        lines = content.split("\n")
        # Check if already ignored (with or without trailing slash)
        if ".kagan" in lines or ".kagan/" in lines:
            return False, False
        # Append to existing file
        if not content.endswith("\n"):
            content += "\n"
        content += "\n# Kagan local state\n.kagan/\n"
        gitignore.write_text(content)
        return False, True
    else:
        # Create new .gitignore
        gitignore.write_text(f"# Kagan local state\n{kagan_entry}\n")
        return True, False


async def init_git_repo(repo_root: Path, base_branch: str) -> GitInitResult:
    """Initialize a git repo with the requested base branch and initial commit.

    Handles four scenarios:
    1. Empty folder, no git repo: Create .gitignore with .kagan/, init repo, commit
    2. Existing git repo with commits and .gitignore: Append .kagan/ if needed, commit
    3. Existing git repo with commits but no .gitignore: Create .gitignore, commit
    4. Existing git repo with NO commits: Add .gitignore, create initial commit

    Creates an initial commit so that worktrees can be created from the base branch.
    Without a commit, `git worktree add -b <branch> <path> <base>` fails with
    'fatal: invalid reference: <base>'.

    Returns GitInitResult with success status and any errors.
    """

    async def run_git(*args: str) -> tuple[int, str, str]:
        try:
            proc = await asyncio.create_subprocess_exec(
                "git",
                *args,
                cwd=repo_root,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            stdout, stderr = await proc.communicate()
            returncode = proc.returncode if proc.returncode is not None else 1
            return returncode, stdout.decode(), stderr.decode()
        except FileNotFoundError:
            return 1, "", "Git is not installed"

    # Check git version first
    version = await get_git_version()
    if version is None:
        return GitInitResult(
            success=False,
            error=GitError(
                error_type="version_low",
                message="Git is not installed",
                details="Please install Git to use Kagan",
            ),
        )

    if version < MIN_GIT_VERSION:
        min_ver = f"{MIN_GIT_VERSION[0]}.{MIN_GIT_VERSION[1]}"
        return GitInitResult(
            success=False,
            error=GitError(
                error_type="version_low",
                message=f"Git version {version} is too old",
                details=f"Kagan requires Git {min_ver}+ for worktree support",
            ),
        )

    # Check if git user is configured (needed for commits)
    user_configured, user_error = await check_git_user_configured()
    if not user_configured:
        return GitInitResult(
            success=False,
            error=GitError(
                error_type="user_not_configured",
                message="Git user not configured",
                details=user_error,
            ),
        )

    # Determine if we have an existing repo and if it has commits
    has_repo = await has_git_repo(repo_root)
    repo_has_commits = has_repo and await has_commits(repo_root)

    # Ensure .gitignore has .kagan/ entry
    gitignore_created, gitignore_updated = _ensure_gitignored(repo_root)

    # If repo exists with commits and .gitignore wasn't modified, nothing to do
    if repo_has_commits and not gitignore_created and not gitignore_updated:
        return GitInitResult(
            success=True,
            gitignore_created=False,
            gitignore_updated=False,
            committed=False,
        )

    if not has_repo:
        # Scenario 1 or 3: No git repo exists, initialize one
        # Try init with -b flag first
        code, _, stderr = await run_git("init", "-b", base_branch)
        if code != 0:
            # Fallback for older git versions without -b support
            code, _, stderr = await run_git("init")
            if code != 0:
                return GitInitResult(
                    success=False,
                    error=GitError(
                        error_type="init_failed",
                        message="Failed to initialize git repository",
                        details=stderr.strip() if stderr else None,
                    ),
                    gitignore_created=gitignore_created,
                    gitignore_updated=gitignore_updated,
                )
            code, _, stderr = await run_git("branch", "-M", base_branch)
            if code != 0:
                return GitInitResult(
                    success=False,
                    error=GitError(
                        error_type="init_failed",
                        message="Failed to rename branch",
                        details=stderr.strip() if stderr else None,
                    ),
                    gitignore_created=gitignore_created,
                    gitignore_updated=gitignore_updated,
                )

    # Stage .gitignore (it always exists at this point)
    code, _, stderr = await run_git("add", ".gitignore")
    if code != 0:
        return GitInitResult(
            success=False,
            error=GitError(
                error_type="commit_failed",
                message="Failed to stage .gitignore",
                details=stderr.strip() if stderr else None,
            ),
            gitignore_created=gitignore_created,
            gitignore_updated=gitignore_updated,
        )

    # Commit the changes
    # Scenario 4: repo exists but has no commits - treat as initial commit
    if not has_repo or not repo_has_commits:
        commit_msg = "Initial commit (kagan)"
    elif gitignore_created:
        commit_msg = "Add .gitignore with kagan exclusion"
    else:
        commit_msg = "Add kagan to .gitignore"

    code, _, stderr = await run_git("commit", "-m", commit_msg)
    if code != 0:
        # Check if it's just "nothing to commit" (gitignore already committed)
        if "nothing to commit" in stderr or "nothing added to commit" in stderr:
            return GitInitResult(
                success=True,
                gitignore_created=gitignore_created,
                gitignore_updated=gitignore_updated,
                committed=False,
            )
        return GitInitResult(
            success=False,
            error=GitError(
                error_type="commit_failed",
                message="Failed to commit .gitignore",
                details=stderr.strip() if stderr else None,
            ),
            gitignore_created=gitignore_created,
            gitignore_updated=gitignore_updated,
        )

    return GitInitResult(
        success=True,
        gitignore_created=gitignore_created,
        gitignore_updated=gitignore_updated,
        committed=True,
    )
