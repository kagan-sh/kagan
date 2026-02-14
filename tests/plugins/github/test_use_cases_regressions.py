"""Regression tests for GitHub plugin use-case edge cases."""

from __future__ import annotations

import json
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

import pytest

from kagan.core.plugins.github.application.use_cases import (
    GH_ISSUE_NUMBER_INVALID,
    GH_PR_NUMBER_INVALID,
    GH_SYNC_FAILED,
    GitHubPluginUseCases,
)
from kagan.core.plugins.github.domain.models import (
    AcquireLeaseInput,
    LinkPrToTaskInput,
    SyncIssuesInput,
)
from kagan.core.plugins.github.gh_adapter import GITHUB_CONNECTION_KEY, GhIssue


def _connected_repo() -> SimpleNamespace:
    return SimpleNamespace(
        id="repo-1",
        path="/tmp/repo",
        scripts={
            GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme", "repo": "widgets"}),
        },
    )


def _core_gateway(repo: SimpleNamespace) -> SimpleNamespace:
    return SimpleNamespace(
        get_project=AsyncMock(return_value=SimpleNamespace(id="project-1")),
        get_project_repos=AsyncMock(return_value=[repo]),
        get_task=AsyncMock(return_value=None),
        create_task=AsyncMock(),
        update_task_fields=AsyncMock(),
        list_workspaces=AsyncMock(return_value=[]),
        update_repo_scripts=AsyncMock(),
    )


@pytest.mark.asyncio()
async def test_sync_issues_does_not_persist_checkpoint_or_mapping_when_any_issue_fails() -> None:
    repo = _connected_repo()
    core_gateway = _core_gateway(repo)
    core_gateway.create_task.side_effect = RuntimeError("simulated projection failure")

    gh_client = SimpleNamespace(
        resolve_gh_cli_path=MagicMock(return_value=("/usr/bin/gh", None)),
        run_gh_issue_list=MagicMock(
            return_value=(
                [
                    {
                        "number": 17,
                        "title": "Projection error",
                        "state": "OPEN",
                        "labels": [],
                        "updatedAt": "2025-01-10T00:00:00Z",
                    }
                ],
                None,
            )
        ),
        parse_issue_list=MagicMock(
            return_value=[
                GhIssue(
                    number=17,
                    title="Projection error",
                    state="OPEN",
                    labels=[],
                    updated_at="2025-01-10T00:00:00Z",
                )
            ]
        ),
    )

    result = await GitHubPluginUseCases(core_gateway, gh_client).sync_issues(
        SyncIssuesInput(project_id="project-1")
    )

    assert result["success"] is False
    assert result["code"] == GH_SYNC_FAILED
    assert result["stats"]["errors"] == 1
    core_gateway.update_repo_scripts.assert_not_awaited()


@pytest.mark.asyncio()
async def test_acquire_lease_returns_structured_error_for_non_numeric_issue_number() -> None:
    core_gateway = _core_gateway(_connected_repo())
    gh_client = SimpleNamespace(resolve_gh_cli_path=MagicMock())

    result = await GitHubPluginUseCases(core_gateway, gh_client).acquire_lease(
        AcquireLeaseInput(project_id="project-1", issue_number="not-a-number")
    )

    assert result["success"] is False
    assert result["code"] == GH_ISSUE_NUMBER_INVALID
    assert "positive integer" in result["message"]
    assert "issue_number" in result["hint"]
    core_gateway.get_project.assert_not_awaited()
    gh_client.resolve_gh_cli_path.assert_not_called()


@pytest.mark.asyncio()
async def test_link_pr_to_task_returns_structured_error_for_non_numeric_pr_number() -> None:
    core_gateway = _core_gateway(_connected_repo())
    gh_client = SimpleNamespace(run_gh_pr_view=MagicMock())

    result = await GitHubPluginUseCases(core_gateway, gh_client).link_pr_to_task(
        LinkPrToTaskInput(
            project_id="project-1",
            task_id="task-1",
            pr_number="1.5",
        )
    )

    assert result["success"] is False
    assert result["code"] == GH_PR_NUMBER_INVALID
    assert "positive integer" in result["message"]
    assert "pr_number" in result["hint"]
    core_gateway.get_project.assert_not_awaited()
    gh_client.run_gh_pr_view.assert_not_called()
