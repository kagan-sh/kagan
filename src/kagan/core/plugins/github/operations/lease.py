"""Lease operation orchestration for the GitHub plugin."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from kagan.core.plugins.github.gh_adapter import run_gh_auth_status
from kagan.core.plugins.github.lease import (
    LEASE_HELD_BY_OTHER,
    acquire_lease,
    get_lease_state,
    release_lease,
)
from kagan.core.plugins.github.operations.common import GH_ISSUE_REQUIRED
from kagan.core.plugins.github.operations.resolver import (
    resolve_connect_target,
    resolve_connected_repo_context,
    resolve_gh_cli_path,
)

if TYPE_CHECKING:
    from kagan.core.bootstrap import AppContext


async def handle_acquire_lease(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Acquire a lease on a GitHub issue for the current Kagan instance."""
    project_id = params.get("project_id")
    repo_id = params.get("repo_id")
    issue_number = params.get("issue_number")
    force_takeover = params.get("force_takeover", False)

    if issue_number is None:
        return {
            "success": False,
            "code": GH_ISSUE_REQUIRED,
            "message": "issue_number is required",
            "hint": "Provide the GitHub issue number to acquire lease for",
        }

    repo, resolve_error = await resolve_connect_target(ctx, project_id, repo_id)
    if resolve_error is not None:
        return resolve_error
    assert repo is not None

    repo_context, connection_error = resolve_connected_repo_context(repo, require_owner_repo=True)
    if connection_error is not None:
        return connection_error
    assert repo_context is not None

    gh_path, gh_error = resolve_gh_cli_path()
    if gh_error is not None:
        return gh_error
    assert gh_path is not None

    auth_status = run_gh_auth_status(gh_path)
    github_user = auth_status.username if auth_status.authenticated else None

    result = acquire_lease(
        gh_path,
        repo.path,
        str(repo_context["owner"]),
        str(repo_context["repo_name"]),
        int(issue_number),
        github_user=github_user,
        force_takeover=bool(force_takeover),
    )

    if result.success:
        return {
            "success": True,
            "code": result.code,
            "message": result.message,
            "holder": result.holder.to_dict() if result.holder else None,
        }

    response: dict[str, Any] = {
        "success": False,
        "code": result.code,
        "message": result.message,
    }
    if result.code == LEASE_HELD_BY_OTHER and result.holder is not None:
        response["holder"] = result.holder.to_dict()
        response["hint"] = (
            f"Issue #{issue_number} is locked by another instance. "
            "Use force_takeover=true to take over the lease."
        )
    return response


async def handle_release_lease(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Release a lease on a GitHub issue."""
    project_id = params.get("project_id")
    repo_id = params.get("repo_id")
    issue_number = params.get("issue_number")

    if issue_number is None:
        return {
            "success": False,
            "code": GH_ISSUE_REQUIRED,
            "message": "issue_number is required",
            "hint": "Provide the GitHub issue number to release lease for",
        }

    repo, resolve_error = await resolve_connect_target(ctx, project_id, repo_id)
    if resolve_error is not None:
        return resolve_error
    assert repo is not None

    repo_context, connection_error = resolve_connected_repo_context(repo, require_owner_repo=True)
    if connection_error is not None:
        return connection_error
    assert repo_context is not None

    gh_path, gh_error = resolve_gh_cli_path()
    if gh_error is not None:
        return gh_error
    assert gh_path is not None

    result = release_lease(
        gh_path,
        repo.path,
        str(repo_context["owner"]),
        str(repo_context["repo_name"]),
        int(issue_number),
    )
    return {
        "success": result.success,
        "code": result.code,
        "message": result.message,
    }


async def handle_get_lease_state(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Get the current lease state for a GitHub issue."""
    project_id = params.get("project_id")
    repo_id = params.get("repo_id")
    issue_number = params.get("issue_number")

    if issue_number is None:
        return {
            "success": False,
            "code": GH_ISSUE_REQUIRED,
            "message": "issue_number is required",
            "hint": "Provide the GitHub issue number to check lease state for",
        }

    repo, resolve_error = await resolve_connect_target(ctx, project_id, repo_id)
    if resolve_error is not None:
        return resolve_error
    assert repo is not None

    repo_context, connection_error = resolve_connected_repo_context(repo, require_owner_repo=True)
    if connection_error is not None:
        return connection_error
    assert repo_context is not None

    gh_path, gh_error = resolve_gh_cli_path()
    if gh_error is not None:
        return gh_error
    assert gh_path is not None

    state, error = get_lease_state(
        gh_path,
        repo.path,
        str(repo_context["owner"]),
        str(repo_context["repo_name"]),
        int(issue_number),
    )

    if error:
        return {
            "success": False,
            "code": "LEASE_STATE_ERROR",
            "message": f"Failed to get lease state: {error}",
        }

    if state is None:
        return {
            "success": False,
            "code": "LEASE_STATE_ERROR",
            "message": "Failed to get lease state",
        }

    return {
        "success": True,
        "code": "LEASE_STATE_OK",
        "state": {
            "is_locked": state.is_locked,
            "is_held_by_current_instance": state.is_held_by_current_instance,
            "can_acquire": state.can_acquire,
            "requires_takeover": state.requires_takeover,
            "holder": state.holder.to_dict() if state.holder else None,
        },
    }


__all__ = [
    "handle_acquire_lease",
    "handle_get_lease_state",
    "handle_release_lease",
]
