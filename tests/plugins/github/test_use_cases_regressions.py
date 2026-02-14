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
    REVIEW_BLOCKED_NO_PR,
    GitHubPluginUseCases,
)
from kagan.core.plugins.github.domain.models import (
    AcquireLeaseInput,
    ConnectRepoInput,
    LinkPrToTaskInput,
    SyncIssuesInput,
    ValidateReviewTransitionInput,
)
from kagan.core.plugins.github.gh_adapter import GITHUB_CONNECTION_KEY, GhIssue, GhRepoView
from kagan.core.plugins.github.sync import GITHUB_ISSUE_MAPPING_KEY, GITHUB_TASK_PR_MAPPING_KEY


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
        get_workspace_repos=AsyncMock(return_value=[]),
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


@pytest.mark.asyncio()
async def test_sync_issues_uses_normalized_project_id_for_task_creation() -> None:
    repo = _connected_repo()
    core_gateway = _core_gateway(repo)
    core_gateway.create_task = AsyncMock(return_value=SimpleNamespace(id="task-1"))

    gh_client = SimpleNamespace(
        resolve_gh_cli_path=MagicMock(return_value=("/usr/bin/gh", None)),
        run_gh_issue_list=MagicMock(
            return_value=(
                [
                    {
                        "number": 11,
                        "title": "Trim IDs",
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
                    number=11,
                    title="Trim IDs",
                    state="OPEN",
                    labels=[],
                    updated_at="2025-01-10T00:00:00Z",
                )
            ]
        ),
    )

    result = await GitHubPluginUseCases(core_gateway, gh_client).sync_issues(
        SyncIssuesInput(project_id="  project-1  ")
    )

    assert result["success"] is True
    assert core_gateway.create_task.await_count == 1
    assert core_gateway.create_task.await_args.kwargs["project_id"] == "project-1"


@pytest.mark.asyncio()
async def test_connect_repo_runs_preflight_in_worker_thread(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    repo = SimpleNamespace(id="repo-1", path="/tmp/repo", scripts={})
    core_gateway = _core_gateway(repo)

    gh_client = SimpleNamespace(
        run_preflight_checks=MagicMock(
            return_value=(
                GhRepoView(
                    host="github.com",
                    owner="acme",
                    name="widgets",
                    full_name="acme/widgets",
                    visibility="PUBLIC",
                    default_branch="main",
                    clone_url="git@github.com:acme/widgets.git",
                ),
                None,
            )
        ),
        build_connection_metadata=MagicMock(
            return_value={"host": "github.com", "owner": "acme", "repo": "widgets"}
        ),
    )

    calls: list[tuple[object, tuple[object, ...]]] = []

    async def fake_to_thread(func, /, *args, **kwargs):
        assert not kwargs
        calls.append((func, args))
        return func(*args)

    monkeypatch.setattr(
        "kagan.core.plugins.github.application.use_cases.asyncio.to_thread",
        fake_to_thread,
    )

    result = await GitHubPluginUseCases(core_gateway, gh_client).connect_repo(
        ConnectRepoInput(project_id="project-1")
    )

    assert result["success"] is True
    assert calls
    assert calls[0] == (gh_client.run_preflight_checks, ("/tmp/repo",))


@pytest.mark.asyncio()
async def test_validate_review_transition_allows_multi_repo_tasks_with_linked_prs() -> None:
    task_id = "task-123"
    repo_a = SimpleNamespace(
        id="repo-a",
        path="/tmp/repo-a",
        scripts={
            GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme", "repo": "repo-a"}),
            GITHUB_TASK_PR_MAPPING_KEY: json.dumps(
                {
                    "task_to_pr": {
                        task_id: {
                            "pr_number": 10,
                            "pr_url": "https://github.com/acme/repo-a/pull/10",
                            "pr_state": "OPEN",
                            "head_branch": "feature/task-123",
                            "base_branch": "main",
                            "linked_at": "2026-01-01T00:00:00+00:00",
                        }
                    }
                }
            ),
        },
    )
    repo_b = SimpleNamespace(
        id="repo-b",
        path="/tmp/repo-b",
        scripts={
            GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme", "repo": "repo-b"}),
            GITHUB_TASK_PR_MAPPING_KEY: json.dumps(
                {
                    "task_to_pr": {
                        task_id: {
                            "pr_number": 11,
                            "pr_url": "https://github.com/acme/repo-b/pull/11",
                            "pr_state": "OPEN",
                            "head_branch": "feature/task-123",
                            "base_branch": "main",
                            "linked_at": "2026-01-01T00:00:00+00:00",
                        }
                    }
                }
            ),
        },
    )
    core_gateway = _core_gateway(repo_a)
    core_gateway.get_project_repos = AsyncMock(return_value=[repo_a, repo_b])
    gh_client = SimpleNamespace()

    result = await GitHubPluginUseCases(core_gateway, gh_client).validate_review_transition(
        ValidateReviewTransitionInput(task_id=task_id, project_id="project-1")
    )

    assert result == {"allowed": True}


@pytest.mark.asyncio()
async def test_validate_review_transition_reports_hint_for_missing_multi_repo_prs() -> None:
    task_id = "task-456"
    repo_a = SimpleNamespace(
        id="repo-a",
        path="/tmp/repo-a",
        scripts={GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme", "repo": "repo-a"})},
    )
    repo_b = SimpleNamespace(
        id="repo-b",
        path="/tmp/repo-b",
        scripts={GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme", "repo": "repo-b"})},
    )
    core_gateway = _core_gateway(repo_a)
    core_gateway.get_project_repos = AsyncMock(return_value=[repo_a, repo_b])
    core_gateway.list_workspaces = AsyncMock(return_value=[SimpleNamespace(id="ws-1")])
    core_gateway.get_workspace_repos = AsyncMock(
        return_value=[{"repo_id": "repo-a"}, {"repo_id": "repo-b"}]
    )
    gh_client = SimpleNamespace()

    result = await GitHubPluginUseCases(core_gateway, gh_client).validate_review_transition(
        ValidateReviewTransitionInput(task_id=task_id, project_id="project-1")
    )

    assert result["allowed"] is False
    assert result["code"] == REVIEW_BLOCKED_NO_PR
    assert "repo-a" in result["message"]
    assert "repo-b" in result["message"]
    assert "repo_id" in result["hint"]


@pytest.mark.asyncio()
async def test_validate_review_transition_runs_lease_checks_in_worker_thread(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    task_id = "task-789"
    repo = SimpleNamespace(
        id="repo-1",
        path="/tmp/repo",
        scripts={
            GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme", "repo": "widgets"}),
            GITHUB_TASK_PR_MAPPING_KEY: json.dumps(
                {
                    "task_to_pr": {
                        task_id: {
                            "pr_number": 12,
                            "pr_url": "https://github.com/acme/widgets/pull/12",
                            "pr_state": "OPEN",
                            "head_branch": "feature/task-789",
                            "base_branch": "main",
                            "linked_at": "2026-01-01T00:00:00+00:00",
                        }
                    }
                }
            ),
            GITHUB_ISSUE_MAPPING_KEY: json.dumps(
                {
                    "issue_to_task": {"42": task_id},
                    "task_to_issue": {task_id: 42},
                }
            ),
        },
    )
    core_gateway = _core_gateway(repo)
    gh_client = SimpleNamespace(
        resolve_gh_cli_path=MagicMock(return_value=("/usr/bin/gh", None)),
        get_lease_state=MagicMock(
            return_value=(
                SimpleNamespace(
                    is_locked=False,
                    is_held_by_current_instance=False,
                    can_acquire=True,
                    requires_takeover=False,
                    holder=None,
                ),
                None,
            )
        ),
    )

    calls: list[tuple[object, tuple[object, ...]]] = []

    async def fake_to_thread(func, /, *args, **kwargs):
        assert not kwargs
        calls.append((func, args))
        return func(*args)

    monkeypatch.setattr(
        "kagan.core.plugins.github.application.use_cases.asyncio.to_thread",
        fake_to_thread,
    )

    result = await GitHubPluginUseCases(core_gateway, gh_client).validate_review_transition(
        ValidateReviewTransitionInput(task_id=task_id, project_id="project-1")
    )

    assert result == {"allowed": True}
    assert calls
    assert calls[0][0] == gh_client.get_lease_state
