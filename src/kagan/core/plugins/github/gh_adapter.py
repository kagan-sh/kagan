"""GitHub CLI adapter for preflight checks and repo metadata extraction."""

from __future__ import annotations

import json
import shutil
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Final

if TYPE_CHECKING:
    from collections.abc import Mapping

from kagan.core.adapters.process import run_exec_capture_sync

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

# gh CLI command fields and limits
GH_REPO_VIEW_FIELDS: Final = "name,owner,url,visibility,defaultBranchRef,sshUrl,isPrivate"
GH_ISSUE_LIST_FIELDS: Final = "number,title,state,labels,updatedAt"
GH_PR_FIELDS: Final = "number,title,state,url,headRefName,baseRefName,isDraft,mergeable"
GH_ISSUE_LIST_LIMIT: Final = 1000
GH_PR_LIST_LIMIT: Final = 10

# Timeout configuration (seconds)
GH_TIMEOUT_VERSION: Final = 10
GH_TIMEOUT_DEFAULT: Final = 30
GH_TIMEOUT_ISSUE_LIST: Final = 60
GH_TIMEOUT_PR_CREATE: Final = 60


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


def _run_gh_command(
    gh_path: str,
    *args: str,
    repo_path: str | None = None,
    timeout_seconds: float = GH_TIMEOUT_DEFAULT,
    timeout_error: str,
    default_error: str,
) -> tuple[str | None, str | None]:
    """Run a gh command and return stdout text or a normalized error message."""
    try:
        result = run_exec_capture_sync(
            gh_path,
            *args,
            cwd=repo_path,
            timeout=timeout_seconds,
        )
    except TimeoutError:
        return None, timeout_error
    except OSError as exc:
        return None, str(exc)

    if result.returncode != 0:
        return None, result.stderr_text().strip() or default_error
    return result.stdout_text(), None


def resolve_connection_repo_name(connection: Mapping[str, Any]) -> str:
    """Return repository name from metadata (`repo` preferred, legacy `name` fallback)."""
    repo_name = connection.get("repo")
    if isinstance(repo_name, str) and repo_name.strip():
        return repo_name.strip()
    legacy_name = connection.get("name")
    if isinstance(legacy_name, str):
        return legacy_name.strip()
    return ""


def normalize_connection_metadata(connection: Mapping[str, Any]) -> dict[str, Any]:
    """Normalize connection metadata to canonical V1 keys."""
    normalized = dict(connection)
    repo_name = resolve_connection_repo_name(normalized)
    if repo_name:
        normalized["repo"] = repo_name
    normalized.pop("name", None)
    return normalized


def load_connection_metadata(connection_raw: object) -> dict[str, Any] | None:
    """Load and normalize connection metadata from Repo.scripts value."""
    data: object = connection_raw
    if isinstance(connection_raw, str):
        try:
            data = json.loads(connection_raw)
        except json.JSONDecodeError:
            return None
    if not isinstance(data, dict):
        return None
    return normalize_connection_metadata(data)


def resolve_gh_cli() -> GhCliAdapterInfo:
    """Check if gh CLI is available and return adapter info."""
    gh_path = shutil.which("gh")
    if gh_path is None:
        return GhCliAdapterInfo(available=False, path=None, version=None)

    output, error = _run_gh_command(
        gh_path,
        "--version",
        timeout_seconds=GH_TIMEOUT_VERSION,
        timeout_error="gh version check timed out",
        default_error="gh version check failed",
    )
    if error:
        return GhCliAdapterInfo(available=False, path=gh_path, version=None)
    version_line = output.strip().split("\n")[0] if output else None
    version = version_line.split()[-1] if version_line else None
    return GhCliAdapterInfo(available=True, path=gh_path, version=version)


def run_gh_auth_status(gh_path: str) -> GhAuthStatus:
    """Run gh auth status and return authentication status."""
    output, error = _run_gh_command(
        gh_path,
        "auth",
        "status",
        timeout_seconds=GH_TIMEOUT_DEFAULT,
        timeout_error="Auth check timed out",
        default_error="Authentication required",
    )
    if error:
        return GhAuthStatus(authenticated=False, username=None, error=error)

    # Parse username from output
    # Output format: "✓ Logged in to github.com account username (..."
    username = None
    for line in output.split("\n"):
        if "Logged in to" in line and "account" in line:
            parts = line.split("account")
            if len(parts) > 1:
                username_part = parts[1].strip().split()[0]
                username = username_part.rstrip("(").strip()
                break
    return GhAuthStatus(authenticated=True, username=username, error=None)


def run_gh_repo_view(gh_path: str, repo_path: str) -> tuple[dict[str, Any] | None, str | None]:
    """Run gh repo view --json and return raw JSON or error message."""
    output, error = _run_gh_command(
        gh_path,
        "repo",
        "view",
        "--json",
        GH_REPO_VIEW_FIELDS,
        repo_path=repo_path,
        timeout_seconds=GH_TIMEOUT_DEFAULT,
        timeout_error="Repo view timed out",
        default_error="Failed to get repo info",
    )
    if error:
        return None, error
    try:
        return json.loads(output), None
    except json.JSONDecodeError as exc:
        return None, f"Invalid JSON response: {exc}"


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
    except (TypeError, ValueError) as e:
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

    return normalize_connection_metadata(
        {
            "host": repo_view.host,
            "owner": repo_view.owner,
            "repo": repo_view.name,
            "full_name": repo_view.full_name,
            "visibility": repo_view.visibility,
            "default_branch": repo_view.default_branch,
            "clone_url": repo_view.clone_url,
            "connected_at": utc_now().isoformat(),
            "connected_by": username,
        }
    )


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
    output, error = _run_gh_command(
        gh_path,
        "issue",
        "list",
        "--state",
        state,
        "--json",
        GH_ISSUE_LIST_FIELDS,
        "--limit",
        str(GH_ISSUE_LIST_LIMIT),
        repo_path=repo_path,
        timeout_seconds=GH_TIMEOUT_ISSUE_LIST,
        timeout_error="Issue list timed out",
        default_error="Failed to list issues",
    )
    if error:
        return None, error
    try:
        return json.loads(output), None
    except json.JSONDecodeError as exc:
        return None, f"Invalid JSON response: {exc}"


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
    output, error = _run_gh_command(
        gh_path,
        "issue",
        "view",
        str(issue_number),
        "--json",
        "number,title,state,labels,comments",
        repo_path=repo_path,
        timeout_seconds=GH_TIMEOUT_DEFAULT,
        timeout_error="Issue view timed out",
        default_error=f"Failed to view issue #{issue_number}",
    )
    if error:
        return None, error
    try:
        return json.loads(output), None
    except json.JSONDecodeError as exc:
        return None, f"Invalid JSON response: {exc}"


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
    _output, error = _run_gh_command(
        gh_path,
        "issue",
        "edit",
        str(issue_number),
        "--add-label",
        label,
        repo_path=repo_path,
        timeout_seconds=GH_TIMEOUT_DEFAULT,
        timeout_error="Label add timed out",
        default_error=f"Failed to add label to issue #{issue_number}",
    )
    if error:
        return False, error
    return True, None


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
    _output, error = _run_gh_command(
        gh_path,
        "issue",
        "edit",
        str(issue_number),
        "--remove-label",
        label,
        repo_path=repo_path,
        timeout_seconds=GH_TIMEOUT_DEFAULT,
        timeout_error="Label remove timed out",
        default_error=f"Failed to remove label from issue #{issue_number}",
    )
    if error:
        return False, error
    return True, None


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
    _output, error = _run_gh_command(
        gh_path,
        "issue",
        "comment",
        str(issue_number),
        "--body",
        body,
        repo_path=repo_path,
        timeout_seconds=GH_TIMEOUT_DEFAULT,
        timeout_error="Comment create timed out",
        default_error=f"Failed to create comment on issue #{issue_number}",
    )
    if error:
        return None, error
    # gh issue comment prints a URL, but this API only needs success/failure.
    return 0, None


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
    endpoint = f"/repos/{owner}/{repo}/issues/{issue_number}/comments"
    output, error = _run_gh_command(
        gh_path,
        "api",
        endpoint,
        repo_path=repo_path,
        timeout_seconds=GH_TIMEOUT_DEFAULT,
        timeout_error="API call timed out",
        default_error="Failed to fetch comments",
    )
    if error:
        return None, error
    try:
        return json.loads(output), None
    except json.JSONDecodeError as exc:
        return None, f"Invalid JSON response: {exc}"


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
    endpoint = f"/repos/{owner}/{repo}/issues/comments/{comment_id}"
    _output, error = _run_gh_command(
        gh_path,
        "api",
        "-X",
        "DELETE",
        endpoint,
        repo_path=repo_path,
        timeout_seconds=GH_TIMEOUT_DEFAULT,
        timeout_error="API call timed out",
        default_error="Failed to delete comment",
    )
    if error:
        return False, error
    return True, None


# --- PR-related gh CLI operations ---

# Error codes for PR operations
GH_PR_CREATE_FAILED: Final = "GH_PR_CREATE_FAILED"
GH_PR_NOT_FOUND: Final = "GH_PR_NOT_FOUND"
GH_PR_LINK_FAILED: Final = "GH_PR_LINK_FAILED"


@dataclass(frozen=True, slots=True)
class GhPullRequest:
    """Normalized GitHub pull request metadata from gh pr view."""

    number: int
    title: str
    state: str  # "OPEN", "CLOSED", "MERGED"
    url: str
    head_branch: str
    base_branch: str
    is_draft: bool
    mergeable: str | None  # "MERGEABLE", "CONFLICTING", "UNKNOWN", None


def run_gh_pr_create(
    gh_path: str,
    repo_path: str,
    *,
    head_branch: str,
    base_branch: str,
    title: str,
    body: str = "",
    draft: bool = False,
) -> tuple[GhPullRequest | None, str | None]:
    """Create a new pull request using gh pr create.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        head_branch: Branch to create PR from.
        base_branch: Branch to merge into.
        title: PR title.
        body: PR body/description.
        draft: Create as draft PR.

    Returns:
        Tuple of (GhPullRequest, None) on success or (None, error_message) on failure.
    """
    cmd_args = [
        "pr",
        "create",
        "--head",
        head_branch,
        "--base",
        base_branch,
        "--title",
        title,
        "--body",
        body,
    ]
    if draft:
        cmd_args.append("--draft")

    output, error = _run_gh_command(
        gh_path,
        *cmd_args,
        repo_path=repo_path,
        timeout_seconds=GH_TIMEOUT_PR_CREATE,
        timeout_error="PR create timed out",
        default_error="Failed to create PR",
    )
    if error:
        return None, error

    # gh pr create outputs the PR URL on success
    pr_url = output.strip()
    # Fetch PR details to return normalized data
    pr_data, _error = run_gh_pr_view_by_url(gh_path, repo_path, pr_url)
    if pr_data is not None:
        return pr_data, None
    # Fallback: construct minimal PR data from URL
    pr_number = _extract_pr_number_from_url(pr_url)
    if pr_number is not None:
        return GhPullRequest(
            number=pr_number,
            title=title,
            state="OPEN",
            url=pr_url,
            head_branch=head_branch,
            base_branch=base_branch,
            is_draft=draft,
            mergeable=None,
        ), None
    return None, f"Created PR but could not parse URL: {pr_url}"


def _extract_pr_number_from_url(url: str) -> int | None:
    """Extract PR number from a GitHub PR URL."""
    import re

    match = re.search(r"/pull/(\d+)", url)
    return int(match.group(1)) if match else None


def run_gh_pr_view(
    gh_path: str,
    repo_path: str,
    pr_number: int,
) -> tuple[GhPullRequest | None, str | None]:
    """Get pull request details by number.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        pr_number: The PR number.

    Returns:
        Tuple of (GhPullRequest, None) on success or (None, error_message) on failure.
    """
    output, error = _run_gh_command(
        gh_path,
        "pr",
        "view",
        str(pr_number),
        "--json",
        GH_PR_FIELDS,
        repo_path=repo_path,
        timeout_seconds=GH_TIMEOUT_DEFAULT,
        timeout_error="PR view timed out",
        default_error=f"Failed to view PR #{pr_number}",
    )
    if error:
        return None, error
    try:
        data = json.loads(output)
    except json.JSONDecodeError as exc:
        return None, f"Invalid JSON response: {exc}"
    return _parse_gh_pr_view(data), None


def run_gh_pr_view_by_url(
    gh_path: str,
    repo_path: str,
    pr_url: str,
) -> tuple[GhPullRequest | None, str | None]:
    """Get pull request details by URL.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        pr_url: The PR URL.

    Returns:
        Tuple of (GhPullRequest, None) on success or (None, error_message) on failure.
    """
    pr_number = _extract_pr_number_from_url(pr_url)
    if pr_number is None:
        return None, f"Could not extract PR number from URL: {pr_url}"
    return run_gh_pr_view(gh_path, repo_path, pr_number)


def run_gh_pr_list_for_branch(
    gh_path: str,
    repo_path: str,
    head_branch: str,
    *,
    state: str = "open",
) -> tuple[list[GhPullRequest] | None, str | None]:
    """List pull requests for a specific head branch.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        head_branch: Branch name to filter by.
        state: PR state filter: "open", "closed", "merged", or "all".

    Returns:
        Tuple of (list of GhPullRequest, None) on success or (None, error_message) on failure.
    """
    output, error = _run_gh_command(
        gh_path,
        "pr",
        "list",
        "--head",
        head_branch,
        "--state",
        state,
        "--json",
        GH_PR_FIELDS,
        "--limit",
        str(GH_PR_LIST_LIMIT),
        repo_path=repo_path,
        timeout_seconds=GH_TIMEOUT_DEFAULT,
        timeout_error="PR list timed out",
        default_error="Failed to list PRs",
    )
    if error:
        return None, error
    try:
        data = json.loads(output)
    except json.JSONDecodeError as exc:
        return None, f"Invalid JSON response: {exc}"
    prs = [_parse_gh_pr_view(pr) for pr in data]
    return prs, None


def _parse_gh_pr_view(data: dict[str, Any]) -> GhPullRequest:
    """Parse gh pr view JSON into GhPullRequest."""
    return GhPullRequest(
        number=int(data.get("number", 0)),
        title=data.get("title", ""),
        state=data.get("state", "OPEN").upper(),
        url=data.get("url", ""),
        head_branch=data.get("headRefName", ""),
        base_branch=data.get("baseRefName", ""),
        is_draft=bool(data.get("isDraft", False)),
        mergeable=data.get("mergeable"),
    )


def run_gh_pr_status(
    gh_path: str,
    repo_path: str,
    pr_number: int,
) -> tuple[str | None, str | None]:
    """Get the merge status of a PR.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        pr_number: The PR number.

    Returns:
        Tuple of (state, error_message). State is "OPEN", "CLOSED", or "MERGED".
    """
    pr_data, error = run_gh_pr_view(gh_path, repo_path, pr_number)
    if error:
        return None, error
    if pr_data is None:
        return None, "No PR data returned"
    return pr_data.state, None


__all__ = [
    "ALREADY_CONNECTED",
    "GH_AUTH_REQUIRED",
    "GH_CLI_NOT_AVAILABLE",
    "GH_PROJECT_REQUIRED",
    "GH_PR_CREATE_FAILED",
    "GH_PR_LINK_FAILED",
    "GH_PR_NOT_FOUND",
    "GH_REPO_ACCESS_DENIED",
    "GH_REPO_METADATA_INVALID",
    "GH_REPO_REQUIRED",
    "GITHUB_CONNECTION_KEY",
    "GhAuthStatus",
    "GhCliAdapterInfo",
    "GhComment",
    "GhIssue",
    "GhPullRequest",
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
    "run_gh_issue_label_add",
    "run_gh_issue_label_remove",
    "run_gh_issue_list",
    "run_gh_issue_view",
    "run_gh_pr_create",
    "run_gh_pr_list_for_branch",
    "run_gh_pr_status",
    "run_gh_pr_view",
    "run_gh_pr_view_by_url",
    "run_gh_repo_view",
    "run_preflight_checks",
]
