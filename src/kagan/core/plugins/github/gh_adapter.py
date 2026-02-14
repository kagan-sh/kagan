"""GitHub CLI adapter for preflight checks and repo metadata extraction."""

from __future__ import annotations

import json
import shutil
from dataclasses import dataclass
from typing import Any, Final

# Error codes for machine-readable responses
GH_CLI_NOT_AVAILABLE: Final = "GH_CLI_NOT_AVAILABLE"
GH_AUTH_REQUIRED: Final = "GH_AUTH_REQUIRED"
GH_REPO_ACCESS_DENIED: Final = "GH_REPO_ACCESS_DENIED"
GH_REPO_METADATA_INVALID: Final = "GH_REPO_METADATA_INVALID"
GH_PROJECT_REQUIRED: Final = "GH_PROJECT_REQUIRED"
GH_REPO_REQUIRED: Final = "GH_REPO_REQUIRED"
ALREADY_CONNECTED: Final = "ALREADY_CONNECTED"

# Connection metadata key stored in Repo.scripts
GITHUB_CONNECTION_KEY: Final = "kagan.github.connection"


@dataclass(frozen=True, slots=True)
class GhCliAdapterInfo:
    """Information about the gh CLI installation."""

    available: bool
    path: str | None
    version: str | None


@dataclass(frozen=True, slots=True)
class GhAuthStatus:
    """Result of gh auth status check."""

    authenticated: bool
    username: str | None
    error: str | None


@dataclass(frozen=True, slots=True)
class GhRepoView:
    """Normalized GitHub repository metadata from gh repo view."""

    host: str
    owner: str
    name: str
    full_name: str
    visibility: str
    default_branch: str
    clone_url: str


@dataclass(frozen=True, slots=True)
class PreflightError:
    """Machine-readable error with remediation hint."""

    code: str
    message: str
    hint: str


def resolve_gh_cli() -> GhCliAdapterInfo:
    """Check if gh CLI is available and return adapter info."""
    gh_path = shutil.which("gh")
    if gh_path is None:
        return GhCliAdapterInfo(available=False, path=None, version=None)

    import subprocess

    try:
        result = subprocess.run(
            [gh_path, "--version"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        version_line = result.stdout.strip().split("\n")[0] if result.stdout else None
        version = version_line.split()[-1] if version_line else None
        return GhCliAdapterInfo(available=True, path=gh_path, version=version)
    except (subprocess.SubprocessError, OSError):
        return GhCliAdapterInfo(available=False, path=gh_path, version=None)


def run_gh_auth_status(gh_path: str) -> GhAuthStatus:
    """Run gh auth status and return authentication status."""
    import subprocess

    try:
        result = subprocess.run(
            [gh_path, "auth", "status"],
            capture_output=True,
            text=True,
            timeout=30,
        )
        if result.returncode == 0:
            # Parse username from output
            # Output format: "✓ Logged in to github.com account username (..."
            output = result.stdout + result.stderr
            username = None
            for line in output.split("\n"):
                if "Logged in to" in line and "account" in line:
                    parts = line.split("account")
                    if len(parts) > 1:
                        username_part = parts[1].strip().split()[0]
                        username = username_part.rstrip("(").strip()
                        break
            return GhAuthStatus(authenticated=True, username=username, error=None)
        return GhAuthStatus(
            authenticated=False,
            username=None,
            error=result.stderr.strip() or "Authentication required",
        )
    except subprocess.TimeoutExpired:
        return GhAuthStatus(authenticated=False, username=None, error="Auth check timed out")
    except (subprocess.SubprocessError, OSError) as e:
        return GhAuthStatus(authenticated=False, username=None, error=str(e))


def run_gh_repo_view(gh_path: str, repo_path: str) -> tuple[dict[str, Any] | None, str | None]:
    """Run gh repo view --json and return raw JSON or error message."""
    import subprocess

    fields = "name,owner,url,visibility,defaultBranchRef,sshUrl,isPrivate"
    try:
        result = subprocess.run(
            [gh_path, "repo", "view", "--json", fields],
            capture_output=True,
            text=True,
            timeout=30,
            cwd=repo_path,
        )
        if result.returncode == 0:
            try:
                return json.loads(result.stdout), None
            except json.JSONDecodeError as e:
                return None, f"Invalid JSON response: {e}"
        return None, result.stderr.strip() or "Failed to get repo info"
    except subprocess.TimeoutExpired:
        return None, "Repo view timed out"
    except (subprocess.SubprocessError, OSError) as e:
        return None, str(e)


def parse_gh_repo_view(raw: dict[str, Any]) -> GhRepoView | PreflightError:
    """Parse raw gh repo view JSON into normalized GhRepoView or error."""
    try:
        owner_data = raw.get("owner", {})
        owner = owner_data.get("login") if isinstance(owner_data, dict) else None
        name = raw.get("name")
        url = raw.get("url", "")

        if not owner or not name:
            return PreflightError(
                code=GH_REPO_METADATA_INVALID,
                message="Missing owner or name in repo metadata",
                hint="Ensure the repository exists and you have access to it.",
            )

        # Extract host from URL (e.g., https://github.com/owner/repo -> github.com)
        host = "github.com"
        if url:
            from urllib.parse import urlparse

            parsed = urlparse(url)
            if parsed.netloc:
                host = parsed.netloc

        # Get default branch
        default_branch_ref = raw.get("defaultBranchRef", {})
        default_branch = (
            default_branch_ref.get("name") if isinstance(default_branch_ref, dict) else None
        )
        if not default_branch:
            default_branch = "main"

        # Determine visibility
        visibility = raw.get("visibility", "").upper()
        if not visibility:
            visibility = "PRIVATE" if raw.get("isPrivate") else "PUBLIC"

        # Build clone URL
        clone_url = raw.get("sshUrl") or url

        return GhRepoView(
            host=host,
            owner=owner,
            name=name,
            full_name=f"{owner}/{name}",
            visibility=visibility,
            default_branch=default_branch,
            clone_url=clone_url,
        )
    except Exception as e:
        return PreflightError(
            code=GH_REPO_METADATA_INVALID,
            message=f"Failed to parse repo metadata: {e}",
            hint="The repository metadata format may be unexpected.",
        )


def run_preflight_checks(
    repo_path: str,
) -> tuple[GhRepoView | None, PreflightError | None]:
    """Run the full preflight check chain: gh CLI -> auth -> repo access.

    Returns (GhRepoView, None) on success or (None, PreflightError) on failure.
    """
    # Step 1: Check gh CLI availability
    cli_info = resolve_gh_cli()
    if not cli_info.available or not cli_info.path:
        return None, PreflightError(
            code=GH_CLI_NOT_AVAILABLE,
            message="GitHub CLI (gh) is not installed or not in PATH",
            hint="Install gh CLI: https://cli.github.com/ or run `brew install gh`",
        )

    # Step 2: Check authentication
    auth_status = run_gh_auth_status(cli_info.path)
    if not auth_status.authenticated:
        return None, PreflightError(
            code=GH_AUTH_REQUIRED,
            message=auth_status.error or "Not authenticated with GitHub",
            hint="Run `gh auth login` to authenticate with GitHub",
        )

    # Step 3: Check repo access
    raw_data, error = run_gh_repo_view(cli_info.path, repo_path)
    if raw_data is None:
        # Determine if it's access denied or other error
        error_lower = (error or "").lower()
        if "not found" in error_lower or "permission" in error_lower or "access" in error_lower:
            return None, PreflightError(
                code=GH_REPO_ACCESS_DENIED,
                message=error or "Cannot access repository",
                hint="Verify you have access to this repository on GitHub",
            )
        return None, PreflightError(
            code=GH_REPO_METADATA_INVALID,
            message=error or "Failed to get repository metadata",
            hint="Check that this directory is a valid git repository linked to GitHub",
        )

    # Step 4: Parse and validate metadata
    result = parse_gh_repo_view(raw_data)
    if isinstance(result, PreflightError):
        return None, result

    return result, None


def build_connection_metadata(repo_view: GhRepoView, username: str | None = None) -> dict[str, Any]:
    """Build the connection metadata dict to store in Repo.scripts."""
    from kagan.core.time import utc_now

    return {
        "host": repo_view.host,
        "owner": repo_view.owner,
        "name": repo_view.name,
        "full_name": repo_view.full_name,
        "visibility": repo_view.visibility,
        "default_branch": repo_view.default_branch,
        "clone_url": repo_view.clone_url,
        "connected_at": utc_now().isoformat(),
        "connected_by": username,
    }


@dataclass(frozen=True, slots=True)
class GhIssue:
    """Normalized GitHub issue metadata from gh issue list."""

    number: int
    title: str
    state: str  # "OPEN" or "CLOSED"
    labels: list[str]
    updated_at: str


def run_gh_issue_list(
    gh_path: str, repo_path: str, *, state: str = "all"
) -> tuple[list[dict[str, Any]] | None, str | None]:
    """Run gh issue list --json and return raw JSON list or error message.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        state: Issue state filter: "open", "closed", or "all".

    Returns:
        Tuple of (issues_list, error_message).
    """
    import subprocess

    fields = "number,title,state,labels,updatedAt"
    try:
        result = subprocess.run(
            [gh_path, "issue", "list", "--state", state, "--json", fields, "--limit", "1000"],
            capture_output=True,
            text=True,
            timeout=60,
            cwd=repo_path,
        )
        if result.returncode == 0:
            try:
                return json.loads(result.stdout), None
            except json.JSONDecodeError as e:
                return None, f"Invalid JSON response: {e}"
        return None, result.stderr.strip() or "Failed to list issues"
    except subprocess.TimeoutExpired:
        return None, "Issue list timed out"
    except (subprocess.SubprocessError, OSError) as e:
        return None, str(e)


def parse_gh_issue_list(raw_issues: list[dict[str, Any]]) -> list[GhIssue]:
    """Parse raw gh issue list JSON into normalized GhIssue list."""
    issues = []
    for raw in raw_issues:
        number = raw.get("number")
        title = raw.get("title", "")
        state = raw.get("state", "OPEN").upper()
        labels_raw = raw.get("labels", [])
        labels = [
            label.get("name", "") if isinstance(label, dict) else str(label) for label in labels_raw
        ]
        updated_at = raw.get("updatedAt", "")
        if number is not None:
            issues.append(
                GhIssue(
                    number=int(number),
                    title=title,
                    state=state,
                    labels=labels,
                    updated_at=updated_at,
                )
            )
    return issues


# --- Lease-related gh CLI operations ---


@dataclass(frozen=True, slots=True)
class GhComment:
    """Normalized GitHub issue comment from gh issue view."""

    id: int
    body: str
    author: str
    created_at: str


def run_gh_issue_view(
    gh_path: str,
    repo_path: str,
    issue_number: int,
) -> tuple[dict[str, Any] | None, str | None]:
    """Run gh issue view --json and return issue data with labels and comments.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        issue_number: The issue number to view.

    Returns:
        Tuple of (issue_data, error_message).
    """
    import subprocess

    fields = "number,title,state,labels,comments"
    try:
        result = subprocess.run(
            [gh_path, "issue", "view", str(issue_number), "--json", fields],
            capture_output=True,
            text=True,
            timeout=30,
            cwd=repo_path,
        )
        if result.returncode == 0:
            try:
                return json.loads(result.stdout), None
            except json.JSONDecodeError as e:
                return None, f"Invalid JSON response: {e}"
        return None, result.stderr.strip() or f"Failed to view issue #{issue_number}"
    except subprocess.TimeoutExpired:
        return None, "Issue view timed out"
    except (subprocess.SubprocessError, OSError) as e:
        return None, str(e)


def parse_gh_issue_comments(raw_comments: list[dict[str, Any]]) -> list[GhComment]:
    """Parse raw gh issue comments JSON into normalized GhComment list."""
    comments = []
    for raw in raw_comments:
        # gh CLI returns comment IDs as strings or integers
        comment_id = raw.get("id")
        if comment_id is None:
            # Try databaseId as fallback
            comment_id = raw.get("databaseId")
        if comment_id is None:
            continue
        body = raw.get("body", "")
        author_data = raw.get("author", {})
        author = author_data.get("login", "") if isinstance(author_data, dict) else ""
        created_at = raw.get("createdAt", "")
        comments.append(
            GhComment(
                id=int(comment_id) if isinstance(comment_id, (int, str)) else 0,
                body=body,
                author=author,
                created_at=created_at,
            )
        )
    return comments


def run_gh_issue_label_add(
    gh_path: str,
    repo_path: str,
    issue_number: int,
    label: str,
) -> tuple[bool, str | None]:
    """Add a label to an issue.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        issue_number: The issue number.
        label: The label to add.

    Returns:
        Tuple of (success, error_message).
    """
    import subprocess

    try:
        result = subprocess.run(
            [gh_path, "issue", "edit", str(issue_number), "--add-label", label],
            capture_output=True,
            text=True,
            timeout=30,
            cwd=repo_path,
        )
        if result.returncode == 0:
            return True, None
        return False, result.stderr.strip() or f"Failed to add label to issue #{issue_number}"
    except subprocess.TimeoutExpired:
        return False, "Label add timed out"
    except (subprocess.SubprocessError, OSError) as e:
        return False, str(e)


def run_gh_issue_label_remove(
    gh_path: str,
    repo_path: str,
    issue_number: int,
    label: str,
) -> tuple[bool, str | None]:
    """Remove a label from an issue.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        issue_number: The issue number.
        label: The label to remove.

    Returns:
        Tuple of (success, error_message).
    """
    import subprocess

    try:
        result = subprocess.run(
            [gh_path, "issue", "edit", str(issue_number), "--remove-label", label],
            capture_output=True,
            text=True,
            timeout=30,
            cwd=repo_path,
        )
        if result.returncode == 0:
            return True, None
        return False, result.stderr.strip() or f"Failed to remove label from issue #{issue_number}"
    except subprocess.TimeoutExpired:
        return False, "Label remove timed out"
    except (subprocess.SubprocessError, OSError) as e:
        return False, str(e)


def run_gh_issue_comment_create(
    gh_path: str,
    repo_path: str,
    issue_number: int,
    body: str,
) -> tuple[int | None, str | None]:
    """Create a comment on an issue.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        issue_number: The issue number.
        body: The comment body.

    Returns:
        Tuple of (comment_id, error_message). comment_id is None on failure.
    """
    import subprocess

    try:
        # Create comment and get the URL back
        result = subprocess.run(
            [gh_path, "issue", "comment", str(issue_number), "--body", body],
            capture_output=True,
            text=True,
            timeout=30,
            cwd=repo_path,
        )
        if result.returncode == 0:
            # gh issue comment outputs the comment URL on success
            # We'll return a placeholder ID since we can't easily get the real ID
            # The comment can be identified by its body content marker
            return 0, None
        return None, result.stderr.strip() or f"Failed to create comment on issue #{issue_number}"
    except subprocess.TimeoutExpired:
        return None, "Comment create timed out"
    except (subprocess.SubprocessError, OSError) as e:
        return None, str(e)


def run_gh_issue_comment_delete(
    gh_path: str,
    repo_path: str,
    comment_url: str,
) -> tuple[bool, str | None]:
    """Delete a comment by URL.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        comment_url: The comment URL (from gh api).

    Returns:
        Tuple of (success, error_message).
    """
    import subprocess

    try:
        result = subprocess.run(
            [gh_path, "api", "-X", "DELETE", comment_url],
            capture_output=True,
            text=True,
            timeout=30,
            cwd=repo_path,
        )
        if result.returncode == 0:
            return True, None
        return False, result.stderr.strip() or "Failed to delete comment"
    except subprocess.TimeoutExpired:
        return False, "Comment delete timed out"
    except (subprocess.SubprocessError, OSError) as e:
        return False, str(e)


def run_gh_api_issue_comments(
    gh_path: str,
    repo_path: str,
    owner: str,
    repo: str,
    issue_number: int,
) -> tuple[list[dict[str, Any]] | None, str | None]:
    """Fetch issue comments via gh api for full comment data including IDs.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        owner: Repository owner.
        repo: Repository name.
        issue_number: The issue number.

    Returns:
        Tuple of (comments_list, error_message).
    """
    import subprocess

    endpoint = f"/repos/{owner}/{repo}/issues/{issue_number}/comments"
    try:
        result = subprocess.run(
            [gh_path, "api", endpoint],
            capture_output=True,
            text=True,
            timeout=30,
            cwd=repo_path,
        )
        if result.returncode == 0:
            try:
                return json.loads(result.stdout), None
            except json.JSONDecodeError as e:
                return None, f"Invalid JSON response: {e}"
        return None, result.stderr.strip() or "Failed to fetch comments"
    except subprocess.TimeoutExpired:
        return None, "API call timed out"
    except (subprocess.SubprocessError, OSError) as e:
        return None, str(e)


def run_gh_api_comment_delete(
    gh_path: str,
    repo_path: str,
    owner: str,
    repo: str,
    comment_id: int,
) -> tuple[bool, str | None]:
    """Delete a comment via gh api.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        owner: Repository owner.
        repo: Repository name.
        comment_id: The comment ID to delete.

    Returns:
        Tuple of (success, error_message).
    """
    import subprocess

    endpoint = f"/repos/{owner}/{repo}/issues/comments/{comment_id}"
    try:
        result = subprocess.run(
            [gh_path, "api", "-X", "DELETE", endpoint],
            capture_output=True,
            text=True,
            timeout=30,
            cwd=repo_path,
        )
        if result.returncode == 0:
            return True, None
        return False, result.stderr.strip() or "Failed to delete comment"
    except subprocess.TimeoutExpired:
        return False, "API call timed out"
    except (subprocess.SubprocessError, OSError) as e:
        return False, str(e)


__all__ = [
    "ALREADY_CONNECTED",
    "GH_AUTH_REQUIRED",
    "GH_CLI_NOT_AVAILABLE",
    "GH_PROJECT_REQUIRED",
    "GH_REPO_ACCESS_DENIED",
    "GH_REPO_METADATA_INVALID",
    "GH_REPO_REQUIRED",
    "GITHUB_CONNECTION_KEY",
    "GhAuthStatus",
    "GhCliAdapterInfo",
    "GhComment",
    "GhIssue",
    "GhRepoView",
    "PreflightError",
    "build_connection_metadata",
    "parse_gh_issue_comments",
    "parse_gh_issue_list",
    "parse_gh_repo_view",
    "resolve_gh_cli",
    "run_gh_api_comment_delete",
    "run_gh_api_issue_comments",
    "run_gh_auth_status",
    "run_gh_issue_comment_create",
    "run_gh_issue_comment_delete",
    "run_gh_issue_label_add",
    "run_gh_issue_label_remove",
    "run_gh_issue_list",
    "run_gh_issue_view",
    "run_gh_repo_view",
    "run_preflight_checks",
]
