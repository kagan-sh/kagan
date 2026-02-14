"""Pull request operation orchestration for the GitHub plugin."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from kagan.core.models.enums import TaskStatus
from kagan.core.plugins.github.gh_adapter import run_gh_pr_create, run_gh_pr_view
from kagan.core.plugins.github.operations.common import (
    GH_PR_CREATE_FAILED,
    GH_PR_NOT_FOUND,
    GH_TASK_REQUIRED,
    GH_WORKSPACE_REQUIRED,
)
from kagan.core.plugins.github.operations.resolver import (
    resolve_connect_target,
    resolve_connected_repo_context,
    resolve_gh_cli_path,
)
from kagan.core.plugins.github.operations.state import upsert_repo_pr_mapping
from kagan.core.plugins.github.sync import load_task_pr_mapping
from kagan.core.time import utc_now

if TYPE_CHECKING:
    from kagan.core.bootstrap import AppContext

GH_PR_NUMBER_REQUIRED = "GH_PR_NUMBER_REQUIRED"
GH_NO_LINKED_PR = "GH_NO_LINKED_PR"
PR_STATUS_RECONCILED = "PR_STATUS_RECONCILED"


async def handle_create_pr_for_task(ctx: AppContext, params: dict[str, Any]) -> dict[str, Any]:
    """Create a PR for a task and link it."""
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

    repo, resolve_error = await resolve_connect_target(ctx, project_id, repo_id)
    if resolve_error is not None:
        return resolve_error
    assert repo is not None

    repo_context, connection_error = resolve_connected_repo_context(repo)
    if connection_error is not None:
        return connection_error
    assert repo_context is not None

    connection = repo_context["connection"]
    base_branch = connection.get("default_branch", "main")

    task = await ctx.task_service.get_task(task_id)
    if task is None:
        return {
            "success": False,
            "code": GH_TASK_REQUIRED,
            "message": f"Task not found: {task_id}",
            "hint": "Verify the task_id exists",
        }

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

    pr_title = title or task.title
    pr_body = body or task.description or ""

    gh_path, gh_error = resolve_gh_cli_path()
    if gh_error is not None:
        return gh_error
    assert gh_path is not None

    pr_data, error = run_gh_pr_create(
        gh_path,
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

    await upsert_repo_pr_mapping(ctx, repo.id, pr_mapping)

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
    """Link an existing PR to a task."""
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
            "code": GH_PR_NUMBER_REQUIRED,
            "message": "pr_number is required",
            "hint": "Provide the PR number to link",
        }

    repo, resolve_error = await resolve_connect_target(ctx, project_id, repo_id)
    if resolve_error is not None:
        return resolve_error
    assert repo is not None

    _, connection_error = resolve_connected_repo_context(repo)
    if connection_error is not None:
        return connection_error

    task = await ctx.task_service.get_task(task_id)
    if task is None:
        return {
            "success": False,
            "code": GH_TASK_REQUIRED,
            "message": f"Task not found: {task_id}",
            "hint": "Verify the task_id exists",
        }

    gh_path, gh_error = resolve_gh_cli_path()
    if gh_error is not None:
        return gh_error
    assert gh_path is not None

    pr_data, error = run_gh_pr_view(gh_path, repo.path, int(pr_number))
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

    await upsert_repo_pr_mapping(ctx, repo.id, pr_mapping)

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
    """Reconcile PR status for a task and apply deterministic board transitions."""
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

    repo, resolve_error = await resolve_connect_target(ctx, project_id, repo_id)
    if resolve_error is not None:
        return resolve_error
    assert repo is not None

    _, connection_error = resolve_connected_repo_context(repo)
    if connection_error is not None:
        return connection_error

    pr_mapping = load_task_pr_mapping(repo.scripts)
    pr_link = pr_mapping.get_pr(task_id)
    if pr_link is None:
        return {
            "success": False,
            "code": GH_NO_LINKED_PR,
            "message": f"Task {task_id} has no linked PR",
            "hint": "Use create_pr_for_task or link_pr_to_task first",
        }

    task = await ctx.task_service.get_task(task_id)
    if task is None:
        return {
            "success": False,
            "code": GH_TASK_REQUIRED,
            "message": f"Task not found: {task_id}",
            "hint": "Verify the task_id exists",
        }

    gh_path, gh_error = resolve_gh_cli_path()
    if gh_error is not None:
        return gh_error
    assert gh_path is not None

    pr_data, error = run_gh_pr_view(gh_path, repo.path, pr_link.pr_number)
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

    pr_state_changed = pr_data.state != pr_link.pr_state
    task_status_changed = False
    previous_task_status = task.status
    new_task_status = task.status

    if pr_state_changed:
        pr_mapping.update_pr_state(task_id, pr_data.state)
        await upsert_repo_pr_mapping(ctx, repo.id, pr_mapping)

    if pr_data.state == "MERGED":
        if task.status != TaskStatus.DONE:
            await ctx.task_service.update_fields(task_id, status=TaskStatus.DONE)
            task_status_changed = True
            new_task_status = TaskStatus.DONE
    elif pr_data.state == "CLOSED":
        if task.status not in {TaskStatus.DONE, TaskStatus.IN_PROGRESS}:
            await ctx.task_service.update_fields(task_id, status=TaskStatus.IN_PROGRESS)
            task_status_changed = True
            new_task_status = TaskStatus.IN_PROGRESS

    return {
        "success": True,
        "code": PR_STATUS_RECONCILED,
        "message": build_reconcile_message(pr_data.number, pr_data.state, task_status_changed),
        "pr": {
            "number": pr_data.number,
            "url": pr_data.url,
            "state": pr_data.state,
            "previous_state": pr_link.pr_state,
            "state_changed": pr_state_changed,
        },
        "task": {
            "id": task_id,
            "status": new_task_status.value,
            "previous_status": previous_task_status.value,
            "status_changed": task_status_changed,
        },
    }


def build_reconcile_message(pr_number: int, pr_state: str, task_changed: bool) -> str:
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


__all__ = [
    "build_reconcile_message",
    "handle_create_pr_for_task",
    "handle_link_pr_to_task",
    "handle_reconcile_pr_status",
]
