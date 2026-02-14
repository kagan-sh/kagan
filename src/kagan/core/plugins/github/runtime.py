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
    load_repo_default_mode,
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
    repo_default_mode = load_repo_default_mode(repo.scripts)

    # Build existing tasks lookup for issues we have mappings for
    existing_tasks = await _load_mapped_tasks(ctx, mapping, project_id)

    # Process each issue
    result = SyncResult(success=True)
    new_mapping = IssueMapping(
        issue_to_task=dict(mapping.issue_to_task),
        task_to_issue=dict(mapping.task_to_issue),
    )

    for issue in issues:
        action, changes = compute_issue_changes(issue, mapping, existing_tasks, repo_default_mode)

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


GH_ISSUE_REQUIRED = "GH_ISSUE_REQUIRED"


async def handle_acquire_lease(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Acquire a lease on a GitHub issue for the current Kagan instance.

    Params:
        project_id: Required project ID
        repo_id: Optional repo ID (required for multi-repo projects)
        issue_number: Required issue number
        force_takeover: Optional bool to force takeover of existing lease

    Returns success with lease holder info or error with current holder info.
    """
    from kagan.core.plugins.github.gh_adapter import run_gh_auth_status
    from kagan.core.plugins.github.lease import (
        LEASE_HELD_BY_OTHER,
        acquire_lease,
    )

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

    # Resolve project and repo
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

    connection = json.loads(connection_raw) if isinstance(connection_raw, str) else connection_raw
    owner = connection.get("owner", "")
    repo_name = connection.get("name", "")

    # Resolve gh CLI
    cli_info = resolve_gh_cli()
    if not cli_info.available or not cli_info.path:
        return {
            "success": False,
            "code": "GH_CLI_NOT_AVAILABLE",
            "message": "GitHub CLI (gh) is not available",
            "hint": "Install gh CLI: https://cli.github.com/",
        }

    # Get authenticated user for attribution
    auth_status = run_gh_auth_status(cli_info.path)
    github_user = auth_status.username if auth_status.authenticated else None

    # Attempt to acquire lease
    result = acquire_lease(
        cli_info.path,
        repo.path,
        owner,
        repo_name,
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

    # Failed - include holder info for blocked case
    response: dict[str, Any] = {
        "success": False,
        "code": result.code,
        "message": result.message,
    }
    if result.code == LEASE_HELD_BY_OTHER and result.holder:
        response["holder"] = result.holder.to_dict()
        response["hint"] = (
            f"Issue #{issue_number} is locked by another instance. "
            "Use force_takeover=true to take over the lease."
        )
    return response


async def handle_release_lease(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Release a lease on a GitHub issue.

    Only succeeds if the current instance holds the lease.

    Params:
        project_id: Required project ID
        repo_id: Optional repo ID (required for multi-repo projects)
        issue_number: Required issue number

    Returns success or error with details.
    """
    from kagan.core.plugins.github.lease import release_lease

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

    # Resolve project and repo
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

    connection = json.loads(connection_raw) if isinstance(connection_raw, str) else connection_raw
    owner = connection.get("owner", "")
    repo_name = connection.get("name", "")

    # Resolve gh CLI
    cli_info = resolve_gh_cli()
    if not cli_info.available or not cli_info.path:
        return {
            "success": False,
            "code": "GH_CLI_NOT_AVAILABLE",
            "message": "GitHub CLI (gh) is not available",
            "hint": "Install gh CLI: https://cli.github.com/",
        }

    # Attempt to release lease
    result = release_lease(
        cli_info.path,
        repo.path,
        owner,
        repo_name,
        int(issue_number),
    )

    return {
        "success": result.success,
        "code": result.code,
        "message": result.message,
    }


async def handle_get_lease_state(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Get the current lease state for a GitHub issue.

    Params:
        project_id: Required project ID
        repo_id: Optional repo ID (required for multi-repo projects)
        issue_number: Required issue number

    Returns lease state with holder info if locked.
    """
    from kagan.core.plugins.github.lease import get_lease_state

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

    # Resolve project and repo
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

    connection = json.loads(connection_raw) if isinstance(connection_raw, str) else connection_raw
    owner = connection.get("owner", "")
    repo_name = connection.get("name", "")

    # Resolve gh CLI
    cli_info = resolve_gh_cli()
    if not cli_info.available or not cli_info.path:
        return {
            "success": False,
            "code": "GH_CLI_NOT_AVAILABLE",
            "message": "GitHub CLI (gh) is not available",
            "hint": "Install gh CLI: https://cli.github.com/",
        }

    # Get lease state
    state, error = get_lease_state(
        cli_info.path,
        repo.path,
        owner,
        repo_name,
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


# --- PR Operations ---

GH_TASK_REQUIRED = "GH_TASK_REQUIRED"
GH_PR_CREATE_FAILED = "GH_PR_CREATE_FAILED"
GH_PR_LINK_FAILED = "GH_PR_LINK_FAILED"
GH_PR_NOT_FOUND = "GH_PR_NOT_FOUND"
GH_WORKSPACE_REQUIRED = "GH_WORKSPACE_REQUIRED"


async def handle_create_pr_for_task(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Create a PR for a task and link it.

    Params:
        project_id: Required project ID
        repo_id: Optional repo ID (required for multi-repo projects)
        task_id: Required task ID
        title: Optional PR title (defaults to task title)
        body: Optional PR body (defaults to task description)
        draft: Optional bool to create as draft PR

    Returns success with PR info or error with details.
    """
    from kagan.core.plugins.github.gh_adapter import run_gh_pr_create
    from kagan.core.plugins.github.sync import (
        load_task_pr_mapping,
    )
    from kagan.core.time import utc_now

    project_id = params.get("project_id")
    repo_id = params.get("repo_id")
    task_id = params.get("task_id")
    title = params.get("title")
    body = params.get("body")
    draft = params.get("draft", False)

    if not task_id:
        return {
            "success": False,
            "code": GH_TASK_REQUIRED,
            "message": "task_id is required",
            "hint": "Provide the task ID to create a PR for",
        }

    # Resolve project and repo
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

    connection = json.loads(connection_raw) if isinstance(connection_raw, str) else connection_raw
    base_branch = connection.get("default_branch", "main")

    # Get task details
    task = await ctx.task_service.get_task(task_id)
    if task is None:
        return {
            "success": False,
            "code": GH_TASK_REQUIRED,
            "message": f"Task not found: {task_id}",
            "hint": "Verify the task_id exists",
        }

    # Get workspace to find the branch
    workspaces = await ctx.workspace_service.list_workspaces(task_id=task_id)
    if not workspaces:
        return {
            "success": False,
            "code": GH_WORKSPACE_REQUIRED,
            "message": "Task has no workspace",
            "hint": "Create a workspace for the task first",
        }

    workspace = workspaces[0]
    head_branch = workspace.branch_name

    # Use task title/description as defaults
    pr_title = title or task.title
    pr_body = body or task.description or ""

    # Resolve gh CLI
    cli_info = resolve_gh_cli()
    if not cli_info.available or not cli_info.path:
        return {
            "success": False,
            "code": "GH_CLI_NOT_AVAILABLE",
            "message": "GitHub CLI (gh) is not available",
            "hint": "Install gh CLI: https://cli.github.com/",
        }

    # Create PR
    pr_data, error = run_gh_pr_create(
        cli_info.path,
        repo.path,
        head_branch=head_branch,
        base_branch=base_branch,
        title=pr_title,
        body=pr_body,
        draft=bool(draft),
    )

    if error:
        return {
            "success": False,
            "code": GH_PR_CREATE_FAILED,
            "message": f"Failed to create PR: {error}",
            "hint": "Check that changes are pushed and the branch exists on GitHub",
        }

    if pr_data is None:
        return {
            "success": False,
            "code": GH_PR_CREATE_FAILED,
            "message": "Failed to create PR: no data returned",
        }

    # Link PR to task
    pr_mapping = load_task_pr_mapping(repo.scripts)
    pr_mapping.link_pr(
        task_id=task_id,
        pr_number=pr_data.number,
        pr_url=pr_data.url,
        pr_state=pr_data.state,
        head_branch=pr_data.head_branch,
        base_branch=pr_data.base_branch,
        linked_at=utc_now().isoformat(),
    )

    # Persist mapping
    await _upsert_repo_pr_mapping(ctx, repo.id, pr_mapping)

    return {
        "success": True,
        "code": "PR_CREATED",
        "message": f"Created PR #{pr_data.number}",
        "pr": {
            "number": pr_data.number,
            "url": pr_data.url,
            "state": pr_data.state,
            "head_branch": pr_data.head_branch,
            "base_branch": pr_data.base_branch,
            "is_draft": pr_data.is_draft,
        },
    }


async def handle_link_pr_to_task(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Link an existing PR to a task.

    Params:
        project_id: Required project ID
        repo_id: Optional repo ID (required for multi-repo projects)
        task_id: Required task ID
        pr_number: Required PR number

    Returns success with PR info or error with details.
    """
    from kagan.core.plugins.github.gh_adapter import run_gh_pr_view
    from kagan.core.plugins.github.sync import (
        load_task_pr_mapping,
    )
    from kagan.core.time import utc_now

    project_id = params.get("project_id")
    repo_id = params.get("repo_id")
    task_id = params.get("task_id")
    pr_number = params.get("pr_number")

    if not task_id:
        return {
            "success": False,
            "code": GH_TASK_REQUIRED,
            "message": "task_id is required",
            "hint": "Provide the task ID to link the PR to",
        }

    if pr_number is None:
        return {
            "success": False,
            "code": "GH_PR_NUMBER_REQUIRED",
            "message": "pr_number is required",
            "hint": "Provide the PR number to link",
        }

    # Resolve project and repo
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

    # Verify task exists
    task = await ctx.task_service.get_task(task_id)
    if task is None:
        return {
            "success": False,
            "code": GH_TASK_REQUIRED,
            "message": f"Task not found: {task_id}",
            "hint": "Verify the task_id exists",
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

    # Get PR details
    pr_data, error = run_gh_pr_view(cli_info.path, repo.path, int(pr_number))
    if error:
        return {
            "success": False,
            "code": GH_PR_NOT_FOUND,
            "message": f"Failed to find PR #{pr_number}: {error}",
            "hint": "Verify the PR exists and you have access to it",
        }

    if pr_data is None:
        return {
            "success": False,
            "code": GH_PR_NOT_FOUND,
            "message": f"PR #{pr_number} not found",
        }

    # Link PR to task
    pr_mapping = load_task_pr_mapping(repo.scripts)
    pr_mapping.link_pr(
        task_id=task_id,
        pr_number=pr_data.number,
        pr_url=pr_data.url,
        pr_state=pr_data.state,
        head_branch=pr_data.head_branch,
        base_branch=pr_data.base_branch,
        linked_at=utc_now().isoformat(),
    )

    # Persist mapping
    await _upsert_repo_pr_mapping(ctx, repo.id, pr_mapping)

    return {
        "success": True,
        "code": "PR_LINKED",
        "message": f"Linked PR #{pr_data.number} to task {task_id}",
        "pr": {
            "number": pr_data.number,
            "url": pr_data.url,
            "state": pr_data.state,
            "head_branch": pr_data.head_branch,
            "base_branch": pr_data.base_branch,
            "is_draft": pr_data.is_draft,
        },
    }


async def handle_reconcile_pr_status(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Reconcile PR status for a task and apply deterministic board transitions.

    This operation is idempotent and safe to re-run. It fetches the current PR
    state from GitHub and updates the task status deterministically:
    - Merged PR -> task moves to DONE
    - Closed (unmerged) PR -> task moves to IN_PROGRESS
    - Open PR -> no task status change

    Params:
        project_id: Required project ID
        repo_id: Optional repo ID (required for multi-repo projects)
        task_id: Required task ID

    Returns success with updated PR and task info or error with retry guidance.
    """
    from kagan.core.models.enums import TaskStatus
    from kagan.core.plugins.github.gh_adapter import run_gh_pr_view
    from kagan.core.plugins.github.sync import load_task_pr_mapping

    project_id = params.get("project_id")
    repo_id = params.get("repo_id")
    task_id = params.get("task_id")

    if not task_id:
        return {
            "success": False,
            "code": GH_TASK_REQUIRED,
            "message": "task_id is required",
            "hint": "Provide the task ID to reconcile PR status for",
        }

    # Resolve project and repo
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

    # Check if task has a linked PR
    pr_mapping = load_task_pr_mapping(repo.scripts)
    pr_link = pr_mapping.get_pr(task_id)

    if pr_link is None:
        return {
            "success": False,
            "code": "GH_NO_LINKED_PR",
            "message": f"Task {task_id} has no linked PR",
            "hint": "Use create_pr_for_task or link_pr_to_task first",
        }

    # Get task to check current status
    task = await ctx.task_service.get_task(task_id)
    if task is None:
        return {
            "success": False,
            "code": GH_TASK_REQUIRED,
            "message": f"Task not found: {task_id}",
            "hint": "Verify the task_id exists",
        }

    # Resolve gh CLI
    cli_info = resolve_gh_cli()
    if not cli_info.available or not cli_info.path:
        return {
            "success": False,
            "code": "GH_CLI_NOT_AVAILABLE",
            "message": "GitHub CLI (gh) is not available",
            "hint": "Install gh CLI: https://cli.github.com/. Retry after installing.",
        }

    # Get current PR status from GitHub
    pr_data, error = run_gh_pr_view(cli_info.path, repo.path, pr_link.pr_number)
    if error:
        return {
            "success": False,
            "code": GH_PR_NOT_FOUND,
            "message": f"Failed to fetch PR #{pr_link.pr_number}: {error}",
            "hint": "Check network connectivity and GitHub access. Retry the reconcile operation.",
        }

    if pr_data is None:
        return {
            "success": False,
            "code": GH_PR_NOT_FOUND,
            "message": f"PR #{pr_link.pr_number} not found",
            "hint": "The PR may have been deleted. Consider unlinking and creating a new PR.",
        }

    # Track changes for response
    pr_state_changed = pr_data.state != pr_link.pr_state
    task_status_changed = False
    previous_task_status = task.status
    new_task_status = task.status

    # Update PR mapping if state changed
    if pr_state_changed:
        pr_mapping.update_pr_state(task_id, pr_data.state)
        await _upsert_repo_pr_mapping(ctx, repo.id, pr_mapping)

    # Deterministic task status transitions based on PR state
    # These transitions are idempotent - running reconcile multiple times
    # produces the same result
    if pr_data.state == "MERGED":
        # Merged PR -> DONE (work is complete and integrated)
        if task.status != TaskStatus.DONE:
            await ctx.task_service.update_fields(task_id, status=TaskStatus.DONE)
            task_status_changed = True
            new_task_status = TaskStatus.DONE
    elif pr_data.state == "CLOSED":
        # Closed without merge -> IN_PROGRESS (work needs attention)
        # Only transition if not already DONE (to avoid overriding completed tasks)
        if task.status != TaskStatus.DONE and task.status != TaskStatus.IN_PROGRESS:
            await ctx.task_service.update_fields(task_id, status=TaskStatus.IN_PROGRESS)
            task_status_changed = True
            new_task_status = TaskStatus.IN_PROGRESS
    # For "OPEN" state, no task status change is applied

    return {
        "success": True,
        "code": "PR_STATUS_RECONCILED",
        "message": _build_reconcile_message(pr_data.number, pr_data.state, task_status_changed),
        "pr": {
            "number": pr_data.number,
            "url": pr_data.url,
            "state": pr_data.state,
            "previous_state": pr_link.pr_state,
            "state_changed": pr_state_changed,
        },
        "task": {
            "id": task_id,
            "status": (
                new_task_status.value
                if hasattr(new_task_status, "value")
                else str(new_task_status)
            ),
            "previous_status": (
                previous_task_status.value
                if hasattr(previous_task_status, "value")
                else str(previous_task_status)
            ),
            "status_changed": task_status_changed,
        },
    }


def _build_reconcile_message(pr_number: int, pr_state: str, task_changed: bool) -> str:
    """Build a human-readable reconcile result message."""
    if pr_state == "MERGED":
        if task_changed:
            return f"PR #{pr_number} merged. Task moved to DONE."
        return f"PR #{pr_number} merged. Task already DONE."
    if pr_state == "CLOSED":
        if task_changed:
            return f"PR #{pr_number} closed without merge. Task moved to IN_PROGRESS."
        return f"PR #{pr_number} closed without merge. Task status unchanged."
    return f"PR #{pr_number} is open. No task status change."


async def _upsert_repo_pr_mapping(
    ctx: AppContext,
    repo_id: str,
    pr_mapping: Any,
) -> None:
    """Persist task-to-PR mapping to Repo.scripts."""
    from kagan.core.adapters.db.schema import Repo
    from kagan.core.adapters.db.session import get_session
    from kagan.core.plugins.github.sync import GITHUB_TASK_PR_MAPPING_KEY

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
        next_scripts[GITHUB_TASK_PR_MAPPING_KEY] = json.dumps(pr_mapping.to_dict())
        repo.scripts = next_scripts

        session.add(repo)
        await session.commit()


__all__ = [
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
