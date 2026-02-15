from __future__ import annotations

from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest

from kagan.core.adapters.db.schema import Task
from kagan.core.models.enums import TaskStatus, TaskType
from kagan.core.services.runtime import RuntimeSessionEvent
from kagan.tui.core_client_api import CoreBackedApi


@pytest.mark.asyncio
async def test_core_backed_api_wait_job_returns_none_on_non_dict_payload(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    call_core = AsyncMock(return_value="not-a-dict")
    monkeypatch.setattr(api, "_call_core", call_core)

    result = await api.wait_job("job-1", task_id="task-1", timeout_seconds=0.25)

    assert result is None
    call_core.assert_awaited_once_with(
        "wait_job",
        kwargs={"job_id": "job-1", "task_id": "task-1", "timeout_seconds": 0.25},
        request_timeout_seconds=5.25,
    )


@pytest.mark.asyncio
async def test_core_backed_api_github_connect_repo_rejects_empty_project_id() -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")

    with pytest.raises(ValueError, match="project_id is required"):
        await api.github_connect_repo(project_id="   ")


@pytest.mark.asyncio
async def test_core_backed_api_github_sync_issues_rejects_blank_repo_id_when_provided() -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")

    with pytest.raises(ValueError, match="repo_id must be a non-empty string when provided"):
        await api.github_sync_issues(project_id="project-1", repo_id="  ")


@pytest.mark.asyncio
async def test_core_backed_api_move_task_returns_none_for_invalid_payload(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    monkeypatch.setattr(api, "_call_core", AsyncMock(return_value="invalid"))

    result = await api.move_task("task-1", TaskStatus.IN_PROGRESS)

    assert result is None


@pytest.mark.asyncio
async def test_core_backed_api_update_task_returns_none_for_invalid_payload(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    monkeypatch.setattr(api, "_call_core", AsyncMock(return_value="invalid"))

    result = await api.update_task("task-1", title="Updated")

    assert result is None


@pytest.mark.asyncio
async def test_core_backed_api_provision_workspace_serializes_only_valid_repo_items(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    class RepoInput:
        def __init__(self, repo_id: str, repo_path: str, target_branch: str) -> None:
            self.repo_id = repo_id
            self.repo_path = repo_path
            self.target_branch = target_branch

    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    call_core = AsyncMock(return_value="ws-1")
    monkeypatch.setattr(api, "_call_core", call_core)

    result = await api.provision_workspace(
        task_id="task-1",
        repos=[
            RepoInput("repo-1", "/tmp/repo-1", "main"),
            {"repo_id": "repo-2", "repo_path": "/tmp/repo-2", "target_branch": "develop"},
            object(),
        ],
    )

    assert result == "ws-1"
    call_core.assert_awaited_once_with(
        "provision_workspace",
        kwargs={
            "task_id": "task-1",
            "repos": [
                {
                    "repo_id": "repo-1",
                    "repo_path": "/tmp/repo-1",
                    "target_branch": "main",
                },
                {
                    "repo_id": "repo-2",
                    "repo_path": "/tmp/repo-2",
                    "target_branch": "develop",
                },
            ],
        },
    )


@pytest.mark.asyncio
async def test_core_backed_api_task_id_extraction_accepts_task_dict_str_and_rejects_invalid(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    call_core = AsyncMock(return_value=True)
    monkeypatch.setattr(api, "_call_core", call_core)

    task = Task.create(
        title="Task",
        description="",
        status=TaskStatus.BACKLOG,
        task_type=TaskType.PAIR,
        project_id="project-1",
    )

    assert await api.has_no_changes("task-1") is True
    assert await api.has_no_changes({"id": "task-2"}) is True
    assert await api.has_no_changes(task) is True

    with pytest.raises(ValueError, match="task_id is required"):
        await api.has_no_changes({"missing": "id"})

    called_ids = [call.kwargs["kwargs"]["task_id"] for call in call_core.await_args_list]
    assert called_ids == ["task-1", "task-2", str(task.id)]


@pytest.mark.asyncio
async def test_core_backed_api_delete_task_and_scratchpad_return_safe_fallbacks(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    monkeypatch.setattr(api, "_call_core", AsyncMock(side_effect=["invalid", None]))

    deleted, message = await api.delete_task("task-1")
    scratchpad = await api.get_scratchpad("task-1")

    assert deleted is False
    assert "delete failed" in message
    assert scratchpad == ""


@pytest.mark.asyncio
async def test_core_backed_api_project_and_repo_queries_fail_closed_on_invalid_payloads(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    monkeypatch.setattr(
        api,
        "_call_core",
        AsyncMock(side_effect=["not-list", "not-a-project", "not-a-repo"]),
    )

    projects = await api.list_projects()
    project = await api.find_project_by_repo_path("/tmp/repo")
    updated_repo = await api.update_repo_default_branch("repo-1", "main")

    assert projects == []
    assert project is None
    assert updated_repo is None


@pytest.mark.asyncio
async def test_core_backed_api_settings_mapping_handles_invalid_shapes(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    monkeypatch.setattr(
        api,
        "_request_core",
        AsyncMock(
            side_effect=[
                {"settings": "not-dict"},
                {"success": True, "message": "ok", "updated": "bad", "settings": {"a": 1}},
            ]
        ),
    )

    settings = await api.get_settings()
    success, message, updated, current = await api.update_settings({"key": "value"})

    assert settings == {}
    assert success is True
    assert message == "ok"
    assert updated == {}
    assert current == {"a": 1}


@pytest.mark.asyncio
async def test_core_backed_api_github_contract_probe_and_sync_validate_payload_shape(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    call_core = AsyncMock(side_effect=["invalid", "invalid"])
    monkeypatch.setattr(api, "_call_core", call_core)

    with pytest.raises(RuntimeError, match="invalid GitHub contract probe payload"):
        await api.github_contract_probe(echo="   ")

    with pytest.raises(RuntimeError, match="invalid GitHub sync payload"):
        await api.github_sync_issues(project_id="project-1")

    # Empty echo is intentionally dropped from forwarded kwargs.
    assert call_core.await_args_list[0].kwargs == {"kwargs": {}}


@pytest.mark.asyncio
async def test_core_backed_api_job_and_session_helpers_handle_nonstandard_payloads(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    call_core = AsyncMock(
        side_effect=[
            {
                "job_id": "job-1",
                "task_id": "task-1",
                "action": "start_agent",
                "status": "queued",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z",
            },
            "invalid",
            "truthy",
            0,
            "  ",
        ]
    )
    monkeypatch.setattr(api, "_call_core", call_core)

    job = await api.submit_job("task-1", "start_agent", arguments={"x": 1})
    cancelled = await api.cancel_job("job-1", task_id="task-1")
    attached = await api.attach_session("task-1")
    exists = await api.session_exists("task-1")
    workspace_path = await api.get_workspace_path("task-1")

    assert job.job_id == "job-1"
    assert attached is True
    assert exists is False
    assert cancelled is None
    assert workspace_path is None


@pytest.mark.asyncio
async def test_core_backed_api_workspace_and_merge_helpers_fail_closed(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    call_core = AsyncMock(
        side_effect=[
            "invalid",
            "invalid",
            "invalid",
            ["unexpected"],
            "invalid",
            "invalid",
            "invalid",
            "invalid",
            "invalid",
            [{"repo_id": "r1", "target_branch": "main", "files": "bad"}],
            "invalid",
        ]
    )
    monkeypatch.setattr(api, "_call_core", call_core)

    repos = await api.get_workspace_repos("ws-1")
    orphaned = await api.cleanup_orphan_workspaces({"task-1"})
    commit_log = await api.get_workspace_commit_log("task-1", base_branch="main")
    rebase_result = await api.rebase_workspace("task-1", "main")
    close_exploratory = await api.close_exploratory("task-1")
    merge_direct = await api.merge_task_direct("task-1")
    rejected = await api.apply_rejection_feedback("task-1", "feedback", "backlog")
    queue_status = await api.get_queue_status("session-1")
    queued = await api.get_queued_messages("session-1")
    all_diffs = await api.get_all_diffs("ws-1")

    assert repos == []
    assert orphaned == []
    assert commit_log == []
    assert rebase_result == (False, "Rebase failed", [])
    assert close_exploratory == (False, "Close exploratory failed")
    assert merge_direct == (False, "Merge failed")
    assert rejected is None
    assert queue_status.has_queued is False
    assert queued == []
    assert all_diffs[0].repo_id == "r1"
    assert all_diffs[0].files == []

    with pytest.raises(RuntimeError, match="invalid repo diff payload"):
        await api.get_repo_diff("ws-1", "repo-1")


@pytest.mark.asyncio
async def test_core_backed_api_queue_and_execution_helpers_map_payloads(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    call_core = AsyncMock(
        side_effect=[
            {"id": "q1", "content": "hello"},
            {"has_queued": True},
            [{"id": "q2", "content": "queued"}],
            {"id": "q3", "content": "taken"},
            True,
            {"id": "exec-1"},
            [{"id": "log-1"}, "invalid"],
            {"id": "exec-2"},
            3,
            {"project_id": "project-1", "repo_id": "repo-1"},
        ]
    )
    monkeypatch.setattr(api, "_call_core", call_core)

    queued = await api.queue_message("session-1", "hello")
    status = await api.get_queue_status("session-1")
    messages = await api.get_queued_messages("session-1")
    taken = await api.take_queued_message("session-1")
    removed = await api.remove_queued_message("session-1", 0)
    execution = await api.get_execution("exec-1")
    logs = await api.get_execution_log_entries("exec-1")
    latest = await api.get_latest_execution_for_task("task-1")
    count = await api.count_executions_for_task("task-1")
    state = await api.dispatch_runtime_session(
        RuntimeSessionEvent.PROJECT_SELECTED,
        project_id="project-1",
    )

    assert queued.id == "q1"
    assert status.has_queued is True
    assert [item.id for item in messages] == ["q2"]
    assert taken.id == "q3"
    assert removed is True
    assert execution.id == "exec-1"
    assert [entry.id for entry in logs] == ["log-1"]
    assert latest.id == "exec-2"
    assert count == 3
    assert state.project_id == "project-1"
    assert state.repo_id == "repo-1"
    assert api.runtime_state.project_id == "project-1"
    assert api.runtime_state.repo_id == "repo-1"


@pytest.mark.asyncio
async def test_core_backed_api_runtime_fallback_helpers_are_stable(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    api = CoreBackedApi(client=SimpleNamespace(), session_id="test-session")
    monkeypatch.setattr(api, "_call_core", AsyncMock(return_value={"id": "execution-1"}))

    assert api.is_automation_running("missing-task") is False
    recovered = await api.recover_stale_auto_output("task-1")

    assert recovered.id == "execution-1"
    assert api.refresh_agent_health() is None
    assert api.is_agent_available() is True
    assert api.get_agent_status_message() is None
