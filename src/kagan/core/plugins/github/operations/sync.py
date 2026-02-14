"""Issue sync operation orchestration for the GitHub plugin."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from kagan.core.plugins.github.gh_adapter import parse_gh_issue_list, run_gh_issue_list
from kagan.core.plugins.github.operations.common import GH_SYNC_FAILED
from kagan.core.plugins.github.operations.resolver import (
    resolve_connect_target,
    resolve_connected_repo_context,
    resolve_gh_cli_path,
)
from kagan.core.plugins.github.operations.state import load_mapped_tasks, upsert_repo_sync_state
from kagan.core.plugins.github.sync import (
    IssueMapping,
    SyncCheckpoint,
    SyncOutcome,
    SyncResult,
    compute_issue_changes,
    filter_issues_since_checkpoint,
    load_checkpoint,
    load_mapping,
    load_repo_default_mode,
)
from kagan.core.time import utc_now

if TYPE_CHECKING:
    from kagan.core.bootstrap import AppContext


async def handle_sync_issues(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Sync GitHub issues to Kagan task projections."""
    project_id = params.get("project_id")
    repo_id = params.get("repo_id")

    repo, resolve_error = await resolve_connect_target(ctx, project_id, repo_id)
    if resolve_error is not None:
        return resolve_error
    assert repo is not None

    _, connection_error = resolve_connected_repo_context(repo)
    if connection_error is not None:
        return connection_error

    gh_path, gh_error = resolve_gh_cli_path()
    if gh_error is not None:
        return gh_error
    assert gh_path is not None

    raw_issues, error = run_gh_issue_list(gh_path, repo.path, state="all")
    if error:
        return {
            "success": False,
            "code": GH_SYNC_FAILED,
            "message": f"Failed to fetch issues: {error}",
            "hint": "Check gh CLI authentication and repository access",
        }

    all_issues = parse_gh_issue_list(raw_issues or [])
    checkpoint = load_checkpoint(repo.scripts)
    issues = filter_issues_since_checkpoint(all_issues, checkpoint)

    mapping = load_mapping(repo.scripts)
    repo_default_mode = load_repo_default_mode(repo.scripts)
    existing_tasks = await load_mapped_tasks(ctx, mapping)

    result = SyncResult(success=True)
    new_mapping = IssueMapping(
        issue_to_task=dict(mapping.issue_to_task),
        task_to_issue=dict(mapping.task_to_issue),
    )

    for issue in issues:
        action, changes = compute_issue_changes(
            issue,
            mapping,
            existing_tasks,
            repo_default_mode,
        )

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
            try:
                task = await ctx.task_service.create_task(
                    title=changes["title"],
                    description=changes["description"],
                    project_id=project_id,
                )
                update_fields: dict[str, Any] = {}
                if changes.get("task_type"):
                    update_fields["task_type"] = changes["task_type"]
                if changes.get("status"):
                    update_fields["status"] = changes["status"]
                if update_fields:
                    await ctx.task_service.update_fields(task.id, **update_fields)
                new_mapping.remove_by_issue(issue.number)
                new_mapping.add_mapping(issue.number, task.id)
                result.add_outcome(
                    SyncOutcome(issue_number=issue.number, action="insert", task_id=task.id)
                )
            except Exception as exc:
                result.add_outcome(
                    SyncOutcome(issue_number=issue.number, action="insert", error=str(exc))
                )
            continue

        task_id = mapping.get_task_id(issue.number)
        if not task_id:
            continue

        try:
            await ctx.task_service.update_fields(task_id, **changes)
            result.add_outcome(
                SyncOutcome(issue_number=issue.number, action=action, task_id=task_id)
            )
        except Exception as exc:
            result.add_outcome(
                SyncOutcome(issue_number=issue.number, action=action, error=str(exc))
            )

    new_checkpoint = SyncCheckpoint(
        last_sync_at=utc_now().isoformat(),
        issue_count=len(issues),
    )
    await upsert_repo_sync_state(ctx, repo.id, new_checkpoint, new_mapping)

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


__all__ = ["handle_sync_issues"]
