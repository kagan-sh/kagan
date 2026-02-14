"""Shared state and persistence helpers for GitHub plugin operation handlers."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

from kagan.core.plugins.github.gh_adapter import GITHUB_CONNECTION_KEY
from kagan.core.plugins.github.sync import (
    GITHUB_ISSUE_MAPPING_KEY,
    GITHUB_SYNC_CHECKPOINT_KEY,
    GITHUB_TASK_PR_MAPPING_KEY,
    IssueMapping,
    SyncCheckpoint,
    TaskPRMapping,
)

if TYPE_CHECKING:
    from kagan.core.bootstrap import AppContext


async def load_mapped_tasks(ctx: AppContext, mapping: IssueMapping) -> dict[str, dict[str, Any]]:
    """Load task data for all tasks in an issue mapping."""
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


async def _upsert_repo_scripts(ctx: AppContext, repo_id: str, updates: dict[str, str]) -> None:
    """Persist script key/value updates to a repo."""
    await ctx.project_service.update_repo_script_values(repo_id, updates)


async def upsert_repo_github_connection(
    ctx: AppContext,
    repo_id: str,
    connection_metadata: dict[str, Any],
) -> None:
    """Persist GitHub connection metadata to Repo.scripts."""
    await _upsert_repo_scripts(
        ctx,
        repo_id,
        {GITHUB_CONNECTION_KEY: json.dumps(connection_metadata)},
    )


async def upsert_repo_sync_state(
    ctx: AppContext,
    repo_id: str,
    checkpoint: SyncCheckpoint,
    mapping: IssueMapping,
) -> None:
    """Persist sync checkpoint and issue mapping to Repo.scripts."""
    await _upsert_repo_scripts(
        ctx,
        repo_id,
        {
            GITHUB_SYNC_CHECKPOINT_KEY: json.dumps(checkpoint.to_dict()),
            GITHUB_ISSUE_MAPPING_KEY: json.dumps(mapping.to_dict()),
        },
    )


async def upsert_repo_pr_mapping(ctx: AppContext, repo_id: str, pr_mapping: TaskPRMapping) -> None:
    """Persist task-to-PR mapping to Repo.scripts."""
    await _upsert_repo_scripts(
        ctx,
        repo_id,
        {GITHUB_TASK_PR_MAPPING_KEY: json.dumps(pr_mapping.to_dict())},
    )


__all__ = [
    "load_mapped_tasks",
    "upsert_repo_github_connection",
    "upsert_repo_pr_mapping",
    "upsert_repo_sync_state",
]
