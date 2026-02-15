"""Behavior-focused tests for GitHub PR reconciliation flows."""

from __future__ import annotations

import json
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

import pytest

from kagan.core.models.enums import TaskStatus
from kagan.core.plugins.github.entrypoints.plugin_handlers import (
    handle_create_pr_for_task,
    handle_reconcile_pr_status,
)
from kagan.core.plugins.github.gh_adapter import GITHUB_CONNECTION_KEY, GhPullRequest
from kagan.core.plugins.github.sync import (
    GITHUB_TASK_PR_MAPPING_KEY,
    TaskPRMapping,
    load_task_pr_mapping,
)


def _connected_repo_with_pr(task_id: str, *, pr_state: str = "OPEN") -> SimpleNamespace:
    mapping = TaskPRMapping()
    mapping.link_pr(
        task_id=task_id,
        pr_number=42,
        pr_url="https://github.com/acme/widgets/pull/42",
        pr_state=pr_state,
        head_branch="feature/task",
        base_branch="main",
        linked_at="2025-01-01T00:00:00Z",
    )
    return SimpleNamespace(
        id="repo-1",
        path="/tmp/repo",
        scripts={
            GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme", "repo": "widgets"}),
            GITHUB_TASK_PR_MAPPING_KEY: json.dumps(mapping.to_dict()),
        },
    )


def _connected_repo_without_pr() -> SimpleNamespace:
    return SimpleNamespace(
        id="repo-1",
        path="/tmp/repo",
        scripts={
            GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme", "repo": "widgets"}),
            GITHUB_TASK_PR_MAPPING_KEY: json.dumps(TaskPRMapping().to_dict()),
        },
    )


def _build_ctx(
    repo: SimpleNamespace,
    task_status: TaskStatus = TaskStatus.REVIEW,
) -> SimpleNamespace:
    task = SimpleNamespace(id="task-1", status=task_status)
    project_service = SimpleNamespace(
        get_project=AsyncMock(return_value=SimpleNamespace(id="project-1")),
        get_project_repos=AsyncMock(return_value=[repo]),
    )
    task_service = SimpleNamespace(
        get_task=AsyncMock(return_value=task),
        update_fields=AsyncMock(return_value=None),
    )
    return SimpleNamespace(project_service=project_service, task_service=task_service)


@pytest.mark.asyncio()
async def test_reconcile_pr_status_moves_task_to_done_on_merge() -> None:
    repo = _connected_repo_with_pr("task-1", pr_state="OPEN")
    ctx = _build_ctx(repo)
    merged_pr = GhPullRequest(
        number=42,
        title="Ship feature",
        state="MERGED",
        url="https://github.com/acme/widgets/pull/42",
        head_branch="feature/task",
        base_branch="main",
        is_draft=False,
        mergeable="MERGEABLE",
    )

    with (
        patch(
            "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.resolve_gh_cli_path",
            return_value=("/usr/bin/gh", None),
        ),
        patch(
            "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.run_gh_pr_view",
            return_value=(merged_pr, None),
        ),
        patch(
            "kagan.core.plugins.github.adapters.core_gateway.AppContextCoreGateway.update_repo_scripts",
            new_callable=AsyncMock,
        ),
    ):
        result = await handle_reconcile_pr_status(
            ctx, {"project_id": "project-1", "task_id": "task-1"}
        )

    assert result["success"] is True
    assert result["code"] == "PR_STATUS_RECONCILED"
    assert result["pr"]["state"] == "MERGED"
    assert result["task"]["status_changed"] is True
    ctx.task_service.update_fields.assert_awaited_once_with("task-1", status=TaskStatus.DONE)


@pytest.mark.asyncio()
async def test_reconcile_pr_status_moves_task_to_in_progress_when_closed_without_merge() -> None:
    repo = _connected_repo_with_pr("task-1", pr_state="OPEN")
    ctx = _build_ctx(repo, task_status=TaskStatus.REVIEW)
    closed_pr = GhPullRequest(
        number=42,
        title="Ship feature",
        state="CLOSED",
        url="https://github.com/acme/widgets/pull/42",
        head_branch="feature/task",
        base_branch="main",
        is_draft=False,
        mergeable="UNKNOWN",
    )

    with (
        patch(
            "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.resolve_gh_cli_path",
            return_value=("/usr/bin/gh", None),
        ),
        patch(
            "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.run_gh_pr_view",
            return_value=(closed_pr, None),
        ),
        patch(
            "kagan.core.plugins.github.adapters.core_gateway.AppContextCoreGateway.update_repo_scripts",
            new_callable=AsyncMock,
        ),
    ):
        result = await handle_reconcile_pr_status(
            ctx, {"project_id": "project-1", "task_id": "task-1"}
        )

    assert result["success"] is True
    assert result["pr"]["state"] == "CLOSED"
    assert result["task"]["status"] == TaskStatus.IN_PROGRESS.value
    assert result["task"]["status_changed"] is True
    ctx.task_service.update_fields.assert_awaited_once_with(
        "task-1",
        status=TaskStatus.IN_PROGRESS,
    )


@pytest.mark.asyncio()
async def test_reconcile_pr_status_returns_error_when_no_linked_pr() -> None:
    repo = _connected_repo_without_pr()
    ctx = _build_ctx(repo)

    result = await handle_reconcile_pr_status(ctx, {"project_id": "project-1", "task_id": "task-1"})

    assert result["success"] is False
    assert result["code"] == "GH_NO_LINKED_PR"
    assert "no linked PR" in result["message"]


def test_load_task_pr_mapping_ignores_invalid_payload() -> None:
    mapping = load_task_pr_mapping({GITHUB_TASK_PR_MAPPING_KEY: "invalid-json"})
    assert mapping.get_pr("task-1") is None


@pytest.mark.asyncio()
async def test_create_pr_for_task_rejects_non_string_title() -> None:
    with pytest.raises(ValueError, match="title must be a string"):
        await handle_create_pr_for_task(
            SimpleNamespace(),
            {
                "project_id": "project-1",
                "repo_id": "repo-1",
                "task_id": "task-1",
                "title": 123,
            },
        )
