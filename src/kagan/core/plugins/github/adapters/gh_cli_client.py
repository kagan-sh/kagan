"""gh CLI-backed implementation of GitHub external client port."""

from __future__ import annotations

from typing import Any

from kagan.core.plugins.github.gh_adapter import (
    GH_CLI_NOT_AVAILABLE,
    GhIssue,
    GhPullRequest,
    GhRepoView,
    PreflightError,
    build_connection_metadata,
    parse_gh_issue_list,
    resolve_gh_cli,
    run_gh_auth_status,
    run_gh_issue_list,
    run_gh_pr_create,
    run_gh_pr_view,
    run_preflight_checks,
)
from kagan.core.plugins.github.lease import (
    LeaseAcquireResult,
    LeaseReleaseResult,
    LeaseState,
    acquire_lease,
    get_lease_state,
    release_lease,
)


class GhCliClientAdapter:
    """Adapter that executes GitHub operations via gh CLI."""

    def build_connection_metadata(
        self,
        repo_view: GhRepoView,
        username: str | None = None,
    ) -> dict[str, Any]:
        return build_connection_metadata(repo_view, username=username)

    def run_preflight_checks(
        self,
        repo_path: str,
    ) -> tuple[GhRepoView | None, PreflightError | None]:
        return run_preflight_checks(repo_path)

    def resolve_gh_cli_path(self) -> tuple[str | None, dict[str, Any] | None]:
        cli_info = resolve_gh_cli()
        if not cli_info.available or not cli_info.path:
            return None, {
                "success": False,
                "code": GH_CLI_NOT_AVAILABLE,
                "message": "GitHub CLI (gh) is not available",
                "hint": "Install gh CLI: https://cli.github.com/",
            }
        return cli_info.path, None

    def run_gh_auth_username(self, gh_path: str) -> str | None:
        auth_status = run_gh_auth_status(gh_path)
        return auth_status.username if auth_status.authenticated else None

    def run_gh_issue_list(
        self,
        gh_path: str,
        repo_path: str,
    ) -> tuple[list[dict[str, Any]] | None, str | None]:
        return run_gh_issue_list(gh_path, repo_path, state="all")

    def parse_issue_list(self, raw_issues: list[dict[str, Any]]) -> list[GhIssue]:
        return parse_gh_issue_list(raw_issues)

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
        return run_gh_pr_create(
            gh_path,
            repo_path,
            head_branch=head_branch,
            base_branch=base_branch,
            title=title,
            body=body,
            draft=draft,
        )

    def run_gh_pr_view(
        self,
        gh_path: str,
        repo_path: str,
        pr_number: int,
    ) -> tuple[GhPullRequest | None, str | None]:
        return run_gh_pr_view(gh_path, repo_path, pr_number)

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
        return acquire_lease(
            gh_path,
            repo_path,
            owner,
            repo_name,
            issue_number,
            github_user=github_user,
            force_takeover=force_takeover,
        )

    def release_lease(
        self,
        gh_path: str,
        repo_path: str,
        owner: str,
        repo_name: str,
        issue_number: int,
    ) -> LeaseReleaseResult:
        return release_lease(
            gh_path,
            repo_path,
            owner,
            repo_name,
            issue_number,
        )

    def get_lease_state(
        self,
        gh_path: str,
        repo_path: str,
        owner: str,
        repo_name: str,
        issue_number: int,
    ) -> tuple[LeaseState | None, str | None]:
        return get_lease_state(
            gh_path,
            repo_path,
            owner,
            repo_name,
            issue_number,
        )


__all__ = ["GhCliClientAdapter"]
