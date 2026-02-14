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
    "GhRepoView",
    "PreflightError",
    "build_connection_metadata",
    "parse_gh_repo_view",
    "resolve_gh_cli",
    "run_gh_auth_status",
    "run_gh_repo_view",
    "run_preflight_checks",
]
