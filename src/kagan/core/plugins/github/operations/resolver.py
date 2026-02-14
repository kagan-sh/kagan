"""Shared resolver helpers for GitHub plugin operation handlers."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from kagan.core.plugins.github.gh_adapter import (
    GH_CLI_NOT_AVAILABLE,
    GH_PROJECT_REQUIRED,
    GH_REPO_METADATA_INVALID,
    GH_REPO_REQUIRED,
    GITHUB_CONNECTION_KEY,
    load_connection_metadata,
    resolve_connection_repo_name,
    resolve_gh_cli,
)
from kagan.core.plugins.github.operations.common import GH_NOT_CONNECTED

if TYPE_CHECKING:
    from kagan.core.bootstrap import AppContext


def _error(code: str, message: str, hint: str) -> dict[str, Any]:
    return {"success": False, "code": code, "message": message, "hint": hint}


async def resolve_connect_target(
    ctx: AppContext,
    project_id: str | None,
    repo_id: str | None,
) -> tuple[Any | None, dict[str, Any] | None]:
    """Resolve target repo for connect/sync/lease/PR operations."""
    if not project_id:
        return None, _error(
            GH_PROJECT_REQUIRED,
            "project_id is required",
            "Provide a valid project_id parameter",
        )

    project = await ctx.project_service.get_project(project_id)
    if not project:
        return None, _error(
            GH_PROJECT_REQUIRED,
            f"Project not found: {project_id}",
            "Verify the project_id exists",
        )

    repos = await ctx.project_service.get_project_repos(project_id)
    if not repos:
        return None, _error(
            GH_REPO_REQUIRED,
            "Project has no repositories",
            "Add a repository to the project first",
        )

    if len(repos) == 1:
        return repos[0], None

    if not repo_id:
        return None, _error(
            GH_REPO_REQUIRED,
            "repo_id required for multi-repo projects",
            f"Project has {len(repos)} repos. Specify repo_id explicitly.",
        )

    target_repo = next((repo for repo in repos if repo.id == repo_id), None)
    if target_repo is None:
        return None, _error(
            GH_REPO_REQUIRED,
            f"Repo not found in project: {repo_id}",
            "Verify the repo_id belongs to this project",
        )

    return target_repo, None


def resolve_connected_repo_context(
    repo: Any,
    *,
    require_owner_repo: bool = False,
) -> tuple[dict[str, Any] | None, dict[str, Any] | None]:
    """Load and validate GitHub connection metadata for a project repo."""
    connection_raw = repo.scripts.get(GITHUB_CONNECTION_KEY) if repo.scripts else None
    if not connection_raw:
        return None, _error(
            GH_NOT_CONNECTED,
            "Repository is not connected to GitHub",
            "Run connect_repo first to establish GitHub connection",
        )

    connection = load_connection_metadata(connection_raw)
    if connection is None:
        return None, _error(
            GH_REPO_METADATA_INVALID,
            "Stored GitHub connection metadata is invalid",
            "Reconnect the repository using connect_repo.",
        )

    context: dict[str, Any] = {"connection": connection}
    if require_owner_repo:
        owner = str(connection.get("owner") or "").strip()
        repo_name = resolve_connection_repo_name(connection)
        if not owner or not repo_name:
            return None, _error(
                GH_REPO_METADATA_INVALID,
                "Stored GitHub connection metadata is incomplete",
                "Reconnect the repository to refresh owner/repo metadata.",
            )
        context["owner"] = owner
        context["repo_name"] = repo_name

    return context, None


def resolve_gh_cli_path() -> tuple[str | None, dict[str, Any] | None]:
    """Resolve a usable gh executable path or return a structured error payload."""
    cli_info = resolve_gh_cli()
    if not cli_info.available or not cli_info.path:
        return None, _error(
            GH_CLI_NOT_AVAILABLE,
            "GitHub CLI (gh) is not available",
            "Install gh CLI: https://cli.github.com/",
        )
    return cli_info.path, None


__all__ = [
    "resolve_connect_target",
    "resolve_connected_repo_context",
    "resolve_gh_cli_path",
]
