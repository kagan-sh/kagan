"""Regression tests for GitHub plugin use-case edge cases."""

from __future__ import annotations

import json
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

import pytest

from kagan.core.models.enums import TaskStatus, TaskType
from kagan.core.plugins.github.application.use_cases import (
    CONNECTED,
    GH_ISSUE_NUMBER_INVALID,
    GH_PR_NUMBER_INVALID,
    GH_REPO_REQUIRED,
    GH_SYNC_FAILED,
    GH_WORKSPACE_REQUIRED,
    PR_CREATED,
    PR_LINKED,
    REVIEW_BLOCKED_LEASE,
    REVIEW_BLOCKED_NO_PR,
    GitHubPluginUseCases,
)
from kagan.core.plugins.github.contract import (
    GITHUB_CAPABILITY,
    GITHUB_METHOD_CONNECT_REPO,
    GITHUB_METHOD_SYNC_ISSUES,
    GITHUB_PLUGIN_ID,
    GITHUB_UI_ACTION_CONNECT_REPO_ID,
    GITHUB_UI_ACTION_SYNC_ISSUES_ID,
    GITHUB_UI_BADGE_CONNECTION_ID,
    GITHUB_UI_FORM_REPO_PICKER_ID,
)
from kagan.core.plugins.github.domain.models import (
    AcquireLeaseInput,
    ConnectRepoInput,
    CreatePrForTaskInput,
    LinkPrToTaskInput,
    ReleaseLeaseInput,
    SyncIssuesInput,
    ValidateReviewTransitionInput,
)
from kagan.core.plugins.github.gh_adapter import (
    GITHUB_CONNECTION_KEY,
    GhIssue,
    GhPullRequest,
    GhRepoView,
)
from kagan.core.plugins.github.lease import LEASE_HELD_BY_OTHER
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
async def test_sync_issues_preserves_successful_mappings_across_partial_failures() -> None:
    repo = _connected_repo()
    core_gateway = _core_gateway(repo)
    stored_tasks: dict[str, SimpleNamespace] = {}
    failed_issue_once = False

    async def create_task(*, title: str, description: str, project_id: str) -> SimpleNamespace:
        del description, project_id
        nonlocal failed_issue_once
        if title.startswith("[GH-18]") and not failed_issue_once:
            failed_issue_once = True
            raise RuntimeError("simulated projection failure")

        issue_number = int(title.split("]")[0].removeprefix("[GH-"))
        task_id = f"task-{issue_number}"
        stored_tasks[task_id] = SimpleNamespace(
            id=task_id,
            title=title,
            status=TaskStatus.BACKLOG,
            task_type=TaskType.PAIR,
        )
        return SimpleNamespace(id=task_id)

    async def get_task(task_id: str) -> SimpleNamespace | None:
        return stored_tasks.get(task_id)

    async def update_repo_scripts(repo_id: str, values: dict[str, str]) -> None:
        assert repo_id == repo.id
        repo.scripts.update(values)

    core_gateway.create_task = AsyncMock(side_effect=create_task)
    core_gateway.get_task = AsyncMock(side_effect=get_task)
    core_gateway.update_repo_scripts = AsyncMock(side_effect=update_repo_scripts)

    gh_client = SimpleNamespace(
        resolve_gh_cli_path=MagicMock(return_value=("/usr/bin/gh", None)),
        run_gh_issue_list=MagicMock(
            return_value=(
                [
                    {
                        "number": 17,
                        "title": "Persist mapping",
                        "state": "OPEN",
                        "labels": [],
                        "updatedAt": "2025-01-10T00:00:00Z",
                    },
                    {
                        "number": 18,
                        "title": "Retry me",
                        "state": "OPEN",
                        "labels": [],
                        "updatedAt": "2025-01-10T00:00:00Z",
                    },
                ],
                None,
            )
        ),
        parse_issue_list=MagicMock(
            return_value=[
                GhIssue(
                    number=17,
                    title="Persist mapping",
                    state="OPEN",
                    labels=[],
                    updated_at="2025-01-10T00:00:00Z",
                ),
                GhIssue(
                    number=18,
                    title="Retry me",
                    state="OPEN",
                    labels=[],
                    updated_at="2025-01-10T00:00:00Z",
                ),
            ]
        ),
    )

    use_cases = GitHubPluginUseCases(core_gateway, gh_client)

    first = await use_cases.sync_issues(SyncIssuesInput(project_id="project-1"))

    assert first["success"] is False
    assert first["code"] == GH_SYNC_FAILED
    assert first["stats"]["errors"] == 1
    assert "17" in json.loads(repo.scripts[GITHUB_ISSUE_MAPPING_KEY])["issue_to_task"]

    second = await use_cases.sync_issues(SyncIssuesInput(project_id="project-1"))

    assert second["success"] is True
    create_titles = [call.kwargs["title"] for call in core_gateway.create_task.await_args_list]
    assert create_titles.count("[GH-17] Persist mapping") == 1
    assert create_titles.count("[GH-18] Retry me") == 2


@pytest.mark.asyncio()
async def test_connect_repo_repairs_invalid_stored_metadata() -> None:
    repo = SimpleNamespace(
        id="repo-1",
        path="/tmp/repo",
        scripts={GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme"})},
    )
    core_gateway = _core_gateway(repo)

    async def update_repo_scripts(repo_id: str, values: dict[str, str]) -> None:
        assert repo_id == repo.id
        repo.scripts.update(values)

    core_gateway.update_repo_scripts = AsyncMock(side_effect=update_repo_scripts)

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

    result = await GitHubPluginUseCases(core_gateway, gh_client).connect_repo(
        ConnectRepoInput(project_id="project-1")
    )

    assert result["success"] is True
    assert result["code"] == CONNECTED
    assert "Repaired invalid" in result["message"]
    assert json.loads(repo.scripts[GITHUB_CONNECTION_KEY])["repo"] == "widgets"


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
async def test_link_pr_to_task_success_persists_mapping_and_returns_pr_payload() -> None:
    repo = _connected_repo()
    core_gateway = _core_gateway(repo)
    core_gateway.get_task = AsyncMock(
        return_value=SimpleNamespace(id="task-1", title="Task", description="")
    )

    async def update_repo_scripts(repo_id: str, values: dict[str, str]) -> None:
        assert repo_id == repo.id
        repo.scripts.update(values)

    core_gateway.update_repo_scripts = AsyncMock(side_effect=update_repo_scripts)

    linked_pr = GhPullRequest(
        number=77,
        title="Task PR",
        state="OPEN",
        url="https://github.com/acme/widgets/pull/77",
        head_branch="feature/task-1",
        base_branch="main",
        is_draft=False,
        mergeable="MERGEABLE",
    )
    gh_client = SimpleNamespace(
        resolve_gh_cli_path=MagicMock(return_value=("/usr/bin/gh", None)),
        run_gh_pr_view=MagicMock(return_value=(linked_pr, None)),
    )

    result = await GitHubPluginUseCases(core_gateway, gh_client).link_pr_to_task(
        LinkPrToTaskInput(project_id="project-1", task_id="task-1", pr_number=77)
    )

    assert result["success"] is True
    assert result["code"] == PR_LINKED
    assert result["pr"]["number"] == 77
    assert result["pr"]["url"] == linked_pr.url

    raw_mapping = repo.scripts.get(GITHUB_TASK_PR_MAPPING_KEY)
    assert isinstance(raw_mapping, str)
    persisted = json.loads(raw_mapping)
    linked = persisted["task_to_pr"]["task-1"]
    assert linked["pr_number"] == 77
    assert linked["pr_url"] == linked_pr.url
    assert linked["pr_state"] == "OPEN"


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
async def test_create_pr_for_task_uses_workspace_matching_requested_repo() -> None:
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
    core_gateway.get_task = AsyncMock(
        return_value=SimpleNamespace(id="task-1", title="Task", description="")
    )
    core_gateway.list_workspaces = AsyncMock(
        return_value=[
            SimpleNamespace(id="ws-a", branch_name="feature/repo-a"),
            SimpleNamespace(id="ws-b", branch_name="feature/repo-b"),
        ]
    )

    async def workspace_repos(workspace_id: str) -> list[dict[str, str]]:
        if workspace_id == "ws-a":
            return [{"repo_id": "repo-a"}]
        if workspace_id == "ws-b":
            return [{"repo_id": "repo-b"}]
        return []

    core_gateway.get_workspace_repos = AsyncMock(side_effect=workspace_repos)

    created_pr = GhPullRequest(
        number=42,
        title="Task",
        state="OPEN",
        url="https://github.com/acme/repo-b/pull/42",
        head_branch="feature/repo-b",
        base_branch="main",
        is_draft=False,
        mergeable="MERGEABLE",
    )
    gh_client = SimpleNamespace(
        resolve_gh_cli_path=MagicMock(return_value=("/usr/bin/gh", None)),
        run_gh_pr_create=MagicMock(return_value=(created_pr, None)),
    )

    result = await GitHubPluginUseCases(core_gateway, gh_client).create_pr_for_task(
        CreatePrForTaskInput(project_id="project-1", repo_id="repo-b", task_id="task-1")
    )

    assert result["success"] is True
    assert result["code"] == PR_CREATED
    assert gh_client.run_gh_pr_create.call_args.args[1] == "/tmp/repo-b"
    assert gh_client.run_gh_pr_create.call_args.kwargs["head_branch"] == "feature/repo-b"


@pytest.mark.asyncio()
async def test_create_pr_for_task_fails_when_no_workspace_matches_repo() -> None:
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
    core_gateway.get_task = AsyncMock(
        return_value=SimpleNamespace(id="task-1", title="Task", description="")
    )
    core_gateway.list_workspaces = AsyncMock(
        return_value=[SimpleNamespace(id="ws-a", branch_name="feature/repo-a")]
    )
    core_gateway.get_workspace_repos = AsyncMock(return_value=[{"repo_id": "repo-a"}])

    gh_client = SimpleNamespace(
        resolve_gh_cli_path=MagicMock(return_value=("/usr/bin/gh", None)),
        run_gh_pr_create=MagicMock(),
    )

    result = await GitHubPluginUseCases(core_gateway, gh_client).create_pr_for_task(
        CreatePrForTaskInput(project_id="project-1", repo_id="repo-b", task_id="task-1")
    )

    assert result["success"] is False
    assert result["code"] == GH_WORKSPACE_REQUIRED
    assert "repo_id repo-b" in result["message"]
    gh_client.run_gh_pr_create.assert_not_called()


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
async def test_validate_review_transition_blocks_when_pr_link_missing_for_multi_repo_task() -> None:
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


@pytest.mark.asyncio()
async def test_validate_review_transition_blocks_when_lease_held_by_other_instance() -> None:
    task_id = "task-lease-conflict"
    repo = SimpleNamespace(
        id="repo-1",
        path="/tmp/repo",
        scripts={
            GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme", "repo": "widgets"}),
            GITHUB_TASK_PR_MAPPING_KEY: json.dumps(
                {
                    "task_to_pr": {
                        task_id: {
                            "pr_number": 55,
                            "pr_url": "https://github.com/acme/widgets/pull/55",
                            "pr_state": "OPEN",
                            "head_branch": "feature/task-lease-conflict",
                            "base_branch": "main",
                            "linked_at": "2026-01-01T00:00:00+00:00",
                        }
                    }
                }
            ),
            GITHUB_ISSUE_MAPPING_KEY: json.dumps(
                {
                    "issue_to_task": {"99": task_id},
                    "task_to_issue": {task_id: 99},
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
                    is_locked=True,
                    is_held_by_current_instance=False,
                    can_acquire=False,
                    requires_takeover=True,
                    holder=SimpleNamespace(instance_id="peer-42"),
                ),
                None,
            )
        ),
    )

    result = await GitHubPluginUseCases(core_gateway, gh_client).validate_review_transition(
        ValidateReviewTransitionInput(task_id=task_id, project_id="project-1")
    )

    assert result["allowed"] is False
    assert result["code"] == REVIEW_BLOCKED_LEASE
    assert "repo-1" in result["message"]
    assert "peer-42" in result["message"]
    assert "another Kagan instance" in result["hint"]


@pytest.mark.asyncio()
async def test_validate_review_transition_blocks_when_lease_metadata_missing_without_takeover() -> (
    None
):
    task_id = "task-lease-metadata-missing"
    repo = SimpleNamespace(
        id="repo-1",
        path="/tmp/repo",
        scripts={
            GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme", "repo": "widgets"}),
            GITHUB_TASK_PR_MAPPING_KEY: json.dumps(
                {
                    "task_to_pr": {
                        task_id: {
                            "pr_number": 91,
                            "pr_url": "https://github.com/acme/widgets/pull/91",
                            "pr_state": "OPEN",
                            "head_branch": "feature/task-lease-metadata-missing",
                            "base_branch": "main",
                            "linked_at": "2026-01-01T00:00:00+00:00",
                        }
                    }
                }
            ),
            GITHUB_ISSUE_MAPPING_KEY: json.dumps(
                {
                    "issue_to_task": {"301": task_id},
                    "task_to_issue": {task_id: 301},
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
                    is_locked=True,
                    is_held_by_current_instance=False,
                    can_acquire=False,
                    requires_takeover=True,
                    holder=None,
                ),
                None,
            )
        ),
    )

    result = await GitHubPluginUseCases(core_gateway, gh_client).validate_review_transition(
        ValidateReviewTransitionInput(task_id=task_id, project_id="project-1")
    )

    assert result["allowed"] is False
    assert result["code"] == REVIEW_BLOCKED_LEASE
    assert "repo-1" in result["message"]
    assert "another Kagan instance" in result["hint"]


@pytest.mark.asyncio()
async def test_release_lease_returns_safe_error_when_not_lease_holder() -> None:
    repo = _connected_repo()
    core_gateway = _core_gateway(repo)
    gh_client = SimpleNamespace(
        resolve_gh_cli_path=MagicMock(return_value=("/usr/bin/gh", None)),
        release_lease=MagicMock(
            return_value=SimpleNamespace(
                success=False,
                code=LEASE_HELD_BY_OTHER,
                message=(
                    "Issue #42 is locked by another instance. Use force_takeover=true to proceed."
                ),
            )
        ),
    )

    result = await GitHubPluginUseCases(core_gateway, gh_client).release_lease(
        ReleaseLeaseInput(project_id="project-1", issue_number=42)
    )

    assert result["success"] is False
    assert result["code"] == LEASE_HELD_BY_OTHER
    assert "another instance" in result["message"]


@pytest.mark.asyncio()
async def test_link_pr_to_task_rejects_repo_mismatch_with_actionable_code() -> None:
    repo = _connected_repo()
    core_gateway = _core_gateway(repo)
    gh_client = SimpleNamespace(run_gh_pr_view=MagicMock())

    result = await GitHubPluginUseCases(core_gateway, gh_client).link_pr_to_task(
        LinkPrToTaskInput(
            project_id="project-1",
            repo_id="repo-mismatch",
            task_id="task-1",
            pr_number=7,
        )
    )

    assert result["success"] is False
    assert result["code"] == GH_REPO_REQUIRED
    assert "Repo not found in project: repo-mismatch" in result["message"]
    assert "single repo" in result["hint"]
    gh_client.run_gh_pr_view.assert_not_called()


@pytest.mark.asyncio()
async def test_ui_describe_exposes_connect_and_sync_actions() -> None:
    from kagan.core.plugins.github.entrypoints.plugin_handlers import handle_ui_describe

    repo_1 = SimpleNamespace(id="repo-1", name="Repo 1", display_name="Repo 1", scripts={})
    repo_2 = SimpleNamespace(id="repo-2", name="Repo 2", display_name="Repo 2", scripts={})
    ctx = SimpleNamespace(
        api=SimpleNamespace(get_project_repos=AsyncMock(return_value=[repo_1, repo_2]))
    )

    payload = await handle_ui_describe(ctx, {"project_id": "project-1", "repo_id": "repo-1"})
    assert payload["schema_version"] == "1"

    actions = payload["actions"]
    assert isinstance(actions, list)
    action_map = {item.get("action_id"): item for item in actions if isinstance(item, dict)}
    assert {GITHUB_UI_ACTION_CONNECT_REPO_ID, GITHUB_UI_ACTION_SYNC_ISSUES_ID} <= set(action_map)

    connect = action_map[GITHUB_UI_ACTION_CONNECT_REPO_ID]
    sync = action_map[GITHUB_UI_ACTION_SYNC_ISSUES_ID]
    for action in (connect, sync):
        assert action["plugin_id"] == GITHUB_PLUGIN_ID
        assert action["surface"] == "kanban.repo_actions"
        assert action["form_id"] == GITHUB_UI_FORM_REPO_PICKER_ID
        assert action["operation"]["capability"] == GITHUB_CAPABILITY

    assert connect["operation"]["method"] == GITHUB_METHOD_CONNECT_REPO
    assert sync["operation"]["method"] == GITHUB_METHOD_SYNC_ISSUES

    forms = payload["forms"]
    assert isinstance(forms, list)
    form = next(
        item
        for item in forms
        if isinstance(item, dict) and item.get("form_id") == GITHUB_UI_FORM_REPO_PICKER_ID
    )
    fields = form["fields"]
    assert isinstance(fields, list)
    repo_field = next(
        item for item in fields if isinstance(item, dict) and item.get("name") == "repo_id"
    )
    assert repo_field["kind"] == "select"
    assert repo_field["required"] is True
    options = repo_field["options"]
    assert isinstance(options, list)
    assert {opt["value"] for opt in options} == {"repo-1", "repo-2"}

    badges = payload["badges"]
    assert isinstance(badges, list)
    badge = next(
        item
        for item in badges
        if isinstance(item, dict) and item.get("badge_id") == GITHUB_UI_BADGE_CONNECTION_ID
    )
    assert badge["plugin_id"] == GITHUB_PLUGIN_ID
    assert badge["surface"] == "header.badges"
    assert badge["label"] == "GitHub"
