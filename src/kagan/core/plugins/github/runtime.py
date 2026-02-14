"""Runtime helper functions for bundled GitHub plugin operations."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

from kagan.core.plugins.github.contract import (
    GITHUB_CANONICAL_METHODS,
    GITHUB_CAPABILITY,
    GITHUB_CONTRACT_PROBE_METHOD,
    GITHUB_CONTRACT_VERSION,
    GITHUB_PLUGIN_ID,
    RESERVED_GITHUB_CAPABILITY,
)
from kagan.core.plugins.github.gh_adapter import (
    ALREADY_CONNECTED,
    GH_PROJECT_REQUIRED,
    GH_REPO_REQUIRED,
    GITHUB_CONNECTION_KEY,
    build_connection_metadata,
    parse_gh_issue_list,
    resolve_gh_cli,
    run_gh_issue_list,
    run_preflight_checks,
)
from kagan.core.plugins.github.sync import (
    GITHUB_ISSUE_MAPPING_KEY,
    GITHUB_SYNC_CHECKPOINT_KEY,
    IssueMapping,
    SyncCheckpoint,
    SyncOutcome,
    SyncResult,
    compute_issue_changes,
    load_checkpoint,
    load_mapping,
)

if TYPE_CHECKING:
    from kagan.core.bootstrap import AppContext


def build_contract_probe_payload(params: dict[str, Any]) -> dict[str, Any]:
    """Return a stable, machine-readable contract response for probe calls."""
    return {
        "success": True,
        "plugin_id": GITHUB_PLUGIN_ID,
        "contract_version": GITHUB_CONTRACT_VERSION,
        "capability": GITHUB_CAPABILITY,
        "method": GITHUB_CONTRACT_PROBE_METHOD,
        "canonical_methods": list(GITHUB_CANONICAL_METHODS),
        "reserved_official_capability": RESERVED_GITHUB_CAPABILITY,
        "echo": params.get("echo"),
    }


async def handle_connect_repo(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Connect a repository to GitHub with preflight checks.

    Params:
        project_id: Required project ID
        repo_id: Optional repo ID (required for multi-repo projects)

    Returns success with connection metadata or error with remediation hint.
    """
    project_id = params.get("project_id")
    repo_id = params.get("repo_id")

    # Resolve project and repo
    resolved = await _resolve_connect_target(ctx, project_id, repo_id)
    if not resolved["success"]:
        return resolved

    repo = resolved["repo"]
    repo_path = repo.path

    # Check if already connected (idempotent)
    existing_connection = repo.scripts.get(GITHUB_CONNECTION_KEY) if repo.scripts else None
    if existing_connection:
        # Already connected - return success with existing metadata
        connection_data = (
            json.loads(existing_connection)
            if isinstance(existing_connection, str)
            else existing_connection
        )
        return {
            "success": True,
            "code": ALREADY_CONNECTED,
            "message": "Repository is already connected to GitHub",
            "connection": connection_data,
        }

    # Run preflight checks
    repo_view, error = run_preflight_checks(repo_path)
    if error:
        return {
            "success": False,
            "code": error.code,
            "message": error.message,
            "hint": error.hint,
        }

    # Build connection metadata
    connection_metadata = build_connection_metadata(repo_view)

    # Persist connection via upsert
    await _upsert_repo_github_connection(ctx, repo.id, connection_metadata)

    return {
        "success": True,
        "code": "CONNECTED",
        "message": f"Connected to {repo_view.full_name}",
        "connection": connection_metadata,
    }


async def _resolve_connect_target(
    ctx: AppContext,
    project_id: str | None,
    repo_id: str | None,
) -> dict[str, Any]:
    """Resolve the target repo for connect operation.

    Single-repo projects auto-resolve; multi-repo requires explicit repo_id.
    """
    if not project_id:
        return {
            "success": False,
            "code": GH_PROJECT_REQUIRED,
            "message": "project_id is required",
            "hint": "Provide a valid project_id parameter",
        }

    project = await ctx.project_service.get_project(project_id)
    if not project:
        return {
            "success": False,
            "code": GH_PROJECT_REQUIRED,
            "message": f"Project not found: {project_id}",
            "hint": "Verify the project_id exists",
        }

    repos = await ctx.project_service.get_project_repos(project_id)
    if not repos:
        return {
            "success": False,
            "code": GH_REPO_REQUIRED,
            "message": "Project has no repositories",
            "hint": "Add a repository to the project first",
        }

    # Single repo: auto-resolve
    if len(repos) == 1:
        return {"success": True, "repo": repos[0]}

    # Multi-repo: require explicit repo_id
    if not repo_id:
        return {
            "success": False,
            "code": GH_REPO_REQUIRED,
            "message": "repo_id required for multi-repo projects",
            "hint": f"Project has {len(repos)} repos. Specify repo_id explicitly.",
        }

    # Find the specified repo
    target_repo = next((r for r in repos if r.id == repo_id), None)
    if not target_repo:
        return {
            "success": False,
            "code": GH_REPO_REQUIRED,
            "message": f"Repo not found in project: {repo_id}",
            "hint": "Verify the repo_id belongs to this project",
        }

    return {"success": True, "repo": target_repo}


async def _upsert_repo_github_connection(
    ctx: AppContext,
    repo_id: str,
    connection_metadata: dict[str, Any],
) -> None:
    """Persist GitHub connection metadata to Repo.scripts."""
    from kagan.core.adapters.db.schema import Repo
    from kagan.core.adapters.db.session import get_session

    # Access the session factory from the task repository (internal implementation detail)
    task_repo = ctx._task_repo
    if task_repo is None:
        msg = "Task repository not initialized"
        raise RuntimeError(msg)

    session_factory = task_repo._session_factory

    async with get_session(session_factory) as session:
        repo = await session.get(Repo, repo_id)
        if repo is None:
            msg = f"Repo not found: {repo_id}"
            raise ValueError(msg)

        # Update scripts with connection metadata (serialize to JSON string)
        next_scripts = dict(repo.scripts) if repo.scripts else {}
        next_scripts[GITHUB_CONNECTION_KEY] = json.dumps(connection_metadata)
        repo.scripts = next_scripts

        session.add(repo)
        await session.commit()


GH_NOT_CONNECTED = "GH_NOT_CONNECTED"
GH_SYNC_FAILED = "GH_SYNC_FAILED"


async def handle_sync_issues(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Sync GitHub issues to Kagan task projections.

    Params:
        project_id: Required project ID
        repo_id: Optional repo ID (required for multi-repo projects)

    Returns success with sync statistics or error with details.
    """
    project_id = params.get("project_id")
    repo_id = params.get("repo_id")

    # Resolve project and repo (reuse connect logic)
    resolved = await _resolve_connect_target(ctx, project_id, repo_id)
    if not resolved["success"]:
        return resolved

    repo = resolved["repo"]

    # Verify GitHub connection exists
    connection_raw = repo.scripts.get(GITHUB_CONNECTION_KEY) if repo.scripts else None
    if not connection_raw:
        return {
            "success": False,
            "code": GH_NOT_CONNECTED,
            "message": "Repository is not connected to GitHub",
            "hint": "Run connect_repo first to establish GitHub connection",
        }

    # Resolve gh CLI
    cli_info = resolve_gh_cli()
    if not cli_info.available or not cli_info.path:
        return {
            "success": False,
            "code": "GH_CLI_NOT_AVAILABLE",
            "message": "GitHub CLI (gh) is not available",
            "hint": "Install gh CLI: https://cli.github.com/",
        }

    # Fetch issues from GitHub
    raw_issues, error = run_gh_issue_list(cli_info.path, repo.path, state="all")
    if error:
        return {
            "success": False,
            "code": GH_SYNC_FAILED,
            "message": f"Failed to fetch issues: {error}",
            "hint": "Check gh CLI authentication and repository access",
        }

    issues = parse_gh_issue_list(raw_issues or [])

    # Load existing mapping (checkpoint loaded for future incremental sync)
    _ = load_checkpoint(repo.scripts)  # Reserved for incremental sync
    mapping = load_mapping(repo.scripts)

    # Build existing tasks lookup for issues we have mappings for
    existing_tasks = await _load_mapped_tasks(ctx, mapping, project_id)

    # Process each issue
    result = SyncResult(success=True)
    new_mapping = IssueMapping(
        issue_to_task=dict(mapping.issue_to_task),
        task_to_issue=dict(mapping.task_to_issue),
    )

    for issue in issues:
        action, changes = compute_issue_changes(issue, mapping, existing_tasks)

        if action == "no_change" or changes is None:
            result.add_outcome(
                SyncOutcome(
                    issue_number=issue.number,
                    action="no_change",
                    task_id=mapping.get_task_id(issue.number),
                )
            )
            continue

        if action == "insert":
            # Create new task
            try:
                task = await ctx.task_service.create_task(
                    title=changes["title"],
                    description=changes["description"],
                    project_id=project_id,
                )
                # Update task type and status if needed
                update_fields: dict[str, Any] = {}
                if changes.get("task_type"):
                    update_fields["task_type"] = changes["task_type"]
                if changes.get("status"):
                    update_fields["status"] = changes["status"]
                if update_fields:
                    await ctx.task_service.update_fields(task.id, **update_fields)
                new_mapping.add_mapping(issue.number, task.id)
                result.add_outcome(
                    SyncOutcome(issue_number=issue.number, action="insert", task_id=task.id)
                )
            except Exception as e:
                result.add_outcome(
                    SyncOutcome(issue_number=issue.number, action="insert", error=str(e))
                )
        else:
            # Update/reopen/close existing task
            task_id = mapping.get_task_id(issue.number)
            if task_id:
                try:
                    await ctx.task_service.update_fields(task_id, **changes)
                    result.add_outcome(
                        SyncOutcome(issue_number=issue.number, action=action, task_id=task_id)
                    )
                except Exception as e:
                    result.add_outcome(
                        SyncOutcome(issue_number=issue.number, action=action, error=str(e))
                    )

    # Update checkpoint and mapping
    from kagan.core.time import utc_now

    new_checkpoint = SyncCheckpoint(
        last_sync_at=utc_now().isoformat(),
        issue_count=len(issues),
    )
    await _upsert_repo_sync_state(ctx, repo.id, new_checkpoint, new_mapping)

    return {
        "success": True,
        "code": "SYNCED",
        "message": f"Synced {len(issues)} issues",
        "stats": {
            "total": len(issues),
            "inserted": result.inserted,
            "updated": result.updated,
            "reopened": result.reopened,
            "closed": result.closed,
            "no_change": result.no_change,
            "errors": result.errors,
        },
    }


async def _load_mapped_tasks(
    ctx: AppContext,
    mapping: IssueMapping,
    project_id: str,
) -> dict[str, dict[str, Any]]:
    """Load task data for all tasks in the mapping."""
    tasks: dict[str, dict[str, Any]] = {}
    for task_id in mapping.task_to_issue:
        task = await ctx.task_service.get_task(task_id)
        if task:
            tasks[task_id] = {
                "title": task.title,
                "status": task.status,
                "task_type": task.task_type,
            }
    return tasks


async def _upsert_repo_sync_state(
    ctx: AppContext,
    repo_id: str,
    checkpoint: SyncCheckpoint,
    mapping: IssueMapping,
) -> None:
    """Persist sync checkpoint and issue mapping to Repo.scripts."""
    from kagan.core.adapters.db.schema import Repo
    from kagan.core.adapters.db.session import get_session

    task_repo = ctx._task_repo
    if task_repo is None:
        msg = "Task repository not initialized"
        raise RuntimeError(msg)

    session_factory = task_repo._session_factory

    async with get_session(session_factory) as session:
        repo = await session.get(Repo, repo_id)
        if repo is None:
            msg = f"Repo not found: {repo_id}"
            raise ValueError(msg)

        next_scripts = dict(repo.scripts) if repo.scripts else {}
        next_scripts[GITHUB_SYNC_CHECKPOINT_KEY] = json.dumps(checkpoint.to_dict())
        next_scripts[GITHUB_ISSUE_MAPPING_KEY] = json.dumps(mapping.to_dict())
        repo.scripts = next_scripts

        session.add(repo)
        await session.commit()


__all__ = ["build_contract_probe_payload", "handle_connect_repo", "handle_sync_issues"]
