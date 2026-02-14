"""Entrypoint handlers for official GitHub plugin operations."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from kagan.core.plugins.github.adapters.core_gateway import AppContextCoreGateway
from kagan.core.plugins.github.adapters.gh_cli_client import GhCliClientAdapter
from kagan.core.plugins.github.application.use_cases import (
    GH_ISSUE_REQUIRED,
    GH_NO_LINKED_PR,
    GH_NOT_CONNECTED,
    GH_PR_CREATE_FAILED,
    GH_PR_NOT_FOUND,
    GH_PR_NUMBER_REQUIRED,
    GH_SYNC_FAILED,
    GH_TASK_REQUIRED,
    GH_WORKSPACE_REQUIRED,
    PR_STATUS_RECONCILED,
    GitHubPluginUseCases,
)
from kagan.core.plugins.github.domain.models import (
    AcquireLeaseInput,
    ConnectRepoInput,
    ContractProbeInput,
    CreatePrForTaskInput,
    GetLeaseStateInput,
    LinkPrToTaskInput,
    ReconcilePrStatusInput,
    ReleaseLeaseInput,
    SyncIssuesInput,
)

if TYPE_CHECKING:
    from kagan.core.bootstrap import AppContext


_GH_CLIENT = GhCliClientAdapter()


def _non_empty_str(value: object) -> str | None:
    if not isinstance(value, str):
        return None
    normalized = value.strip()
    return normalized if normalized else None


def _build_use_cases(ctx: AppContext) -> GitHubPluginUseCases:
    return GitHubPluginUseCases(AppContextCoreGateway(ctx), _GH_CLIENT)


def build_contract_probe_payload(params: dict[str, Any]) -> dict[str, Any]:
    """Return a stable, machine-readable contract response for probe calls."""
    request = ContractProbeInput(echo=params.get("echo"))
    return GitHubPluginUseCases.build_contract_probe_payload(request)


async def handle_connect_repo(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Connect a repository to GitHub with preflight checks."""
    request = ConnectRepoInput(
        project_id=_non_empty_str(params.get("project_id")),
        repo_id=_non_empty_str(params.get("repo_id")),
    )
    return await _build_use_cases(ctx).connect_repo(request)


async def handle_sync_issues(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Sync GitHub issues to Kagan task projections."""
    request = SyncIssuesInput(
        project_id=_non_empty_str(params.get("project_id")),
        repo_id=_non_empty_str(params.get("repo_id")),
    )
    return await _build_use_cases(ctx).sync_issues(request)


async def handle_acquire_lease(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Acquire a lease on a GitHub issue for the current Kagan instance."""
    request = AcquireLeaseInput(
        project_id=_non_empty_str(params.get("project_id")),
        repo_id=_non_empty_str(params.get("repo_id")),
        issue_number=params.get("issue_number"),
        force_takeover=bool(params.get("force_takeover", False)),
    )
    return await _build_use_cases(ctx).acquire_lease(request)


async def handle_release_lease(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Release a lease on a GitHub issue."""
    request = ReleaseLeaseInput(
        project_id=_non_empty_str(params.get("project_id")),
        repo_id=_non_empty_str(params.get("repo_id")),
        issue_number=params.get("issue_number"),
    )
    return await _build_use_cases(ctx).release_lease(request)


async def handle_get_lease_state(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Get the current lease state for a GitHub issue."""
    request = GetLeaseStateInput(
        project_id=_non_empty_str(params.get("project_id")),
        repo_id=_non_empty_str(params.get("repo_id")),
        issue_number=params.get("issue_number"),
    )
    return await _build_use_cases(ctx).get_lease_state(request)


async def handle_create_pr_for_task(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Create a PR for a task and link it."""
    request = CreatePrForTaskInput(
        project_id=_non_empty_str(params.get("project_id")),
        repo_id=_non_empty_str(params.get("repo_id")),
        task_id=_non_empty_str(params.get("task_id")),
        title=params.get("title"),
        body=params.get("body"),
        draft=bool(params.get("draft", False)),
    )
    return await _build_use_cases(ctx).create_pr_for_task(request)


async def handle_link_pr_to_task(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Link an existing PR to a task."""
    request = LinkPrToTaskInput(
        project_id=_non_empty_str(params.get("project_id")),
        repo_id=_non_empty_str(params.get("repo_id")),
        task_id=_non_empty_str(params.get("task_id")),
        pr_number=params.get("pr_number"),
    )
    return await _build_use_cases(ctx).link_pr_to_task(request)


async def handle_reconcile_pr_status(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Reconcile PR status for a task and apply deterministic board transitions."""
    request = ReconcilePrStatusInput(
        project_id=_non_empty_str(params.get("project_id")),
        repo_id=_non_empty_str(params.get("repo_id")),
        task_id=_non_empty_str(params.get("task_id")),
    )
    return await _build_use_cases(ctx).reconcile_pr_status(request)


__all__ = [
    "GH_ISSUE_REQUIRED",
    "GH_NOT_CONNECTED",
    "GH_NO_LINKED_PR",
    "GH_PR_CREATE_FAILED",
    "GH_PR_NOT_FOUND",
    "GH_PR_NUMBER_REQUIRED",
    "GH_SYNC_FAILED",
    "GH_TASK_REQUIRED",
    "GH_WORKSPACE_REQUIRED",
    "PR_STATUS_RECONCILED",
    "build_contract_probe_payload",
    "handle_acquire_lease",
    "handle_connect_repo",
    "handle_create_pr_for_task",
    "handle_get_lease_state",
    "handle_link_pr_to_task",
    "handle_reconcile_pr_status",
    "handle_release_lease",
    "handle_sync_issues",
]
