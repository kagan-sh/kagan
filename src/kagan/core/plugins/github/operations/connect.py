"""Connect operation orchestration for the GitHub plugin."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

from kagan.core.plugins.github.gh_adapter import (
    ALREADY_CONNECTED,
    GH_REPO_METADATA_INVALID,
    GITHUB_CONNECTION_KEY,
    build_connection_metadata,
    load_connection_metadata,
    run_preflight_checks,
)
from kagan.core.plugins.github.operations.resolver import resolve_connect_target
from kagan.core.plugins.github.operations.state import upsert_repo_github_connection

if TYPE_CHECKING:
    from kagan.core.bootstrap import AppContext


async def handle_connect_repo(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Connect a repository to GitHub with preflight checks."""
    project_id = params.get("project_id")
    repo_id = params.get("repo_id")

    repo, resolve_error = await resolve_connect_target(ctx, project_id, repo_id)
    if resolve_error is not None:
        return resolve_error
    assert repo is not None

    existing_connection = repo.scripts.get(GITHUB_CONNECTION_KEY) if repo.scripts else None
    if existing_connection:
        raw_connection_data: object = existing_connection
        if isinstance(existing_connection, str):
            try:
                raw_connection_data = json.loads(existing_connection)
            except json.JSONDecodeError:
                raw_connection_data = None

        connection_data = load_connection_metadata(raw_connection_data)
        if connection_data is None:
            return {
                "success": False,
                "code": GH_REPO_METADATA_INVALID,
                "message": "Stored GitHub connection metadata is invalid",
                "hint": "Reconnect the repository using connect_repo to refresh metadata.",
            }

        if isinstance(raw_connection_data, dict) and raw_connection_data != connection_data:
            await upsert_repo_github_connection(ctx, repo.id, connection_data)

        return {
            "success": True,
            "code": ALREADY_CONNECTED,
            "message": "Repository is already connected to GitHub",
            "connection": connection_data,
        }

    repo_view, error = run_preflight_checks(repo.path)
    if error:
        return {
            "success": False,
            "code": error.code,
            "message": error.message,
            "hint": error.hint,
        }

    connection_metadata = build_connection_metadata(repo_view)
    await upsert_repo_github_connection(ctx, repo.id, connection_metadata)

    return {
        "success": True,
        "code": "CONNECTED",
        "message": f"Connected to {repo_view.full_name}",
        "connection": connection_metadata,
    }


__all__ = ["handle_connect_repo"]
