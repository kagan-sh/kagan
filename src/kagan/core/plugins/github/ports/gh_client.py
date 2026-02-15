"""GitHub external client port for plugin use cases."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Protocol

if TYPE_CHECKING:
    from kagan.core.plugins.github.gh_adapter import (
        GhIssue,
        GhPullRequest,
        GhRepoView,
        PreflightError,
    )
    from kagan.core.plugins.github.lease import (
        LeaseAcquireResult,
        LeaseReleaseResult,
        LeaseState,
    )


class GitHubClient(Protocol):
    """Port for GitHub/gh CLI operations."""

    def build_connection_metadata(
        self,
        repo_view: GhRepoView,
        username: str | None = None,
    ) -> dict[str, Any]:
        """Build persisted connection metadata from a repo view."""
        ...

    def run_preflight_checks(
        self,
        repo_path: str,
    ) -> tuple[GhRepoView | None, PreflightError | None]:
        """Run gh availability/auth/repo preflight checks."""
        ...

    def resolve_gh_cli_path(self) -> tuple[str | None, dict[str, Any] | None]:
        """Resolve executable gh path or return structured error payload."""
        ...

    def run_gh_auth_username(self, gh_path: str) -> str | None:
        """Return authenticated GitHub username, if available."""
        ...

    def run_gh_issue_list(
        self,
        gh_path: str,
        repo_path: str,
    ) -> tuple[list[dict[str, Any]] | None, str | None]:
        """Fetch raw issue list payload from GitHub."""
        ...

    def parse_issue_list(self, raw_issues: list[dict[str, Any]]) -> list[GhIssue]:
        """Parse gh issue payload into domain issue records."""
        ...

    def run_gh_pr_create(
        self,
        gh_path: str,
        repo_path: str,
        *,
        head_branch: str,
        base_branch: str,
        title: str,
        body: str,
        draft: bool,
    ) -> tuple[GhPullRequest | None, str | None]:
        """Create PR and return normalized PR data."""
        ...

    def run_gh_pr_view(
        self,
        gh_path: str,
        repo_path: str,
        pr_number: int,
    ) -> tuple[GhPullRequest | None, str | None]:
        """Fetch PR details by number."""
        ...

    def acquire_lease(
        self,
        gh_path: str,
        repo_path: str,
        owner: str,
        repo_name: str,
        issue_number: int,
        *,
        github_user: str | None,
        force_takeover: bool,
    ) -> LeaseAcquireResult:
        """Acquire issue lease."""
        ...

    def release_lease(
        self,
        gh_path: str,
        repo_path: str,
        owner: str,
        repo_name: str,
        issue_number: int,
    ) -> LeaseReleaseResult:
        """Release issue lease."""
        ...

    def get_lease_state(
        self,
        gh_path: str,
        repo_path: str,
        owner: str,
        repo_name: str,
        issue_number: int,
    ) -> tuple[LeaseState | None, str | None]:
        """Read issue lease state."""
        ...


__all__ = ["GitHubClient"]
