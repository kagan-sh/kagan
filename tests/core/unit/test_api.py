"""Unit tests for KaganAPI -- typed orchestration API."""

from __future__ import annotations

import json
from types import SimpleNamespace
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock, patch

import pytest
from _api_helpers import build_api

from kagan.core.api_tasks import ReviewApprovalContextMissingError
from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
from kagan.core.plugins.github.gh_adapter import GITHUB_CONNECTION_KEY
from kagan.core.plugins.github.sync import (
    GITHUB_ISSUE_MAPPING_KEY,
    GITHUB_LEASE_ENFORCEMENT_KEY,
    GITHUB_TASK_PR_MAPPING_KEY,
)

if TYPE_CHECKING:
    from collections.abc import AsyncGenerator
    from pathlib import Path

    from kagan.core.adapters.db.repositories import TaskRepository
    from kagan.core.api import KaganAPI
    from kagan.core.bootstrap import AppContext


# ── Fixture ───────────────────────────────────────────────────────────


@pytest.fixture
async def handle_env(
    tmp_path: Path,
) -> AsyncGenerator[tuple[TaskRepository, KaganAPI, AppContext]]:
    """Build api with real task/project services and mocked externals."""
    repo, api, ctx = await build_api(tmp_path)
    yield repo, api, ctx
    await repo.close()


# ── Task Group ────────────────────────────────────────────────────────


class TestTaskOperations:
    """Tests for task CRUD and related operations."""

    async def test_create_task(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task("My Task", "A description")
        assert task.title == "My Task"
        assert task.description == "A description"
        assert task.status == TaskStatus.BACKLOG

    async def test_create_task_with_overrides(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task(
            "Priority Task",
            priority=TaskPriority.HIGH,
            task_type=TaskType.AUTO,
            acceptance_criteria=["Must pass tests"],
        )
        assert task.priority == TaskPriority.HIGH
        assert task.task_type == TaskType.AUTO
        assert task.acceptance_criteria == ["Must pass tests"]

    async def test_get_task(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        created = await api.create_task("Get Me")
        fetched = await api.get_task(created.id)
        assert fetched is not None
        assert fetched.id == created.id
        assert fetched.title == "Get Me"

    async def test_get_task_not_found(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        result = await api.get_task("nonexistent-id")
        assert result is None

    async def test_list_tasks(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        await api.create_task("Task A")
        await api.create_task("Task B")
        tasks = await api.list_tasks()
        assert len(tasks) >= 2
        titles = {t.title for t in tasks}
        assert "Task A" in titles
        assert "Task B" in titles

    async def test_list_tasks_with_status_filter(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task("Review Task")
        await api.move_task(task.id, TaskStatus.REVIEW)
        review_tasks = await api.list_tasks(status=TaskStatus.REVIEW)
        assert any(t.id == task.id for t in review_tasks)
        backlog_tasks = await api.list_tasks(status=TaskStatus.BACKLOG)
        assert not any(t.id == task.id for t in backlog_tasks)

    async def test_update_task(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task("Original Title")
        updated = await api.update_task(task.id, title="Updated Title")
        assert updated is not None
        assert updated.title == "Updated Title"

    async def test_update_task_not_found(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        result = await api.update_task("nonexistent-id", title="Nope")
        assert result is None

    async def test_move_task(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task("Move Me")
        moved = await api.move_task(task.id, TaskStatus.IN_PROGRESS)
        assert moved is not None
        assert moved.status == TaskStatus.IN_PROGRESS

    async def test_move_task_to_done_is_rejected(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task("No Direct Done")
        await api.move_task(task.id, TaskStatus.REVIEW)
        with pytest.raises(ValueError, match="Direct move/update to DONE is not allowed"):
            await api.move_task(task.id, TaskStatus.DONE)

    async def test_delete_task(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task("Delete Me")
        success, message = await api.delete_task(task.id)
        assert success is True
        assert "Deleted" in message

    async def test_delete_task_not_found(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        success, message = await api.delete_task("nonexistent-id")
        assert success is False
        assert "not found" in message

    async def test_scratchpad(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task("Scratch Task")
        await api.update_scratchpad(task.id, "Line one")
        content = await api.get_scratchpad(task.id)
        assert "Line one" in content

        await api.update_scratchpad(task.id, "Line two")
        content = await api.get_scratchpad(task.id)
        assert "Line one" in content
        assert "Line two" in content

    async def test_search_tasks(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        await api.create_task("Unique Search Target")
        results = await api.search_tasks("Unique Search")
        assert len(results) >= 1
        assert any(t.title == "Unique Search Target" for t in results)

    async def test_search_tasks_empty_query(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        results = await api.search_tasks("")
        assert len(results) == 0

    async def test_get_task_context(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task("Context Task", "Some description")
        context = await api.get_task_context(task.id)
        assert context["task_id"] == task.id
        assert context["title"] == "Context Task"
        assert context["description"] == "Some description"
        assert "scratchpad" in context
        assert "linked_tasks" in context

    async def test_get_task_context_not_found(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        context = await api.get_task_context("nonexistent")
        assert context["found"] is False


# ── Review Group ──────────────────────────────────────────────────────


class TestReviewOperations:
    """Tests for review workflow operations."""

    async def test_request_review(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task("Review Task")
        reviewed = await api.request_review(task.id, summary="Done!")
        assert reviewed is not None
        assert reviewed.status == TaskStatus.REVIEW

    async def test_approve_task(self, handle_env: tuple) -> None:
        _repo, api, ctx = handle_env
        task = await api.create_task("Approve Task")
        await api.request_review(task.id)
        execution = SimpleNamespace(id="exec-1", metadata_={})
        ctx.execution_service.get_latest_execution_for_task = AsyncMock(return_value=execution)
        ctx.execution_service.update_execution = AsyncMock(return_value=execution)
        approved = await api.approve_task(task.id)
        assert approved is not None
        assert approved.status == TaskStatus.REVIEW

    async def test_approve_task_fails_without_execution_context(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task("Approve Task Missing Context")
        await api.request_review(task.id)

        with pytest.raises(ReviewApprovalContextMissingError, match="no execution context exists"):
            await api.approve_task(task.id)

    async def test_request_review_uses_plugin_guardrail_hooks(self, handle_env: tuple) -> None:
        """Review guardrails should use plugin method hooks, not hardcoded plugin capability."""
        _repo, api, ctx = handle_env
        task = await api.create_task("Plugin hook guardrail")

        async def _guardrail_handler(_ctx, params):
            assert params["task_id"] == task.id
            return {
                "allowed": False,
                "code": "THIRDPARTY_BLOCKED",
                "message": "Blocked by external review policy.",
                "hint": "Resolve external policy requirement and retry.",
            }

        operation = SimpleNamespace(plugin_id="thirdparty.guardrails", handler=_guardrail_handler)

        def _operations_for_method(method: str) -> tuple[object, ...]:
            if method != "validate_review_transition":
                return ()
            return (operation,)

        ctx.plugin_registry = SimpleNamespace(operations_for_method=_operations_for_method)

        with pytest.raises(ValueError, match="Blocked by external review policy"):
            await api.request_review(task.id)

    async def test_request_review_uses_owning_repo_guardrails_only(self, handle_env: tuple) -> None:
        """Multi-repo guardrails should not block a task because of an unrelated repo."""
        _repo, api, ctx = handle_env
        task = await api.create_task("Scoped Guardrail Task")
        pr_mapping = {
            "task_to_pr": {
                task.id: {
                    "pr_number": 7,
                    "pr_url": "https://github.com/acme/repo-a/pull/7",
                    "pr_state": "OPEN",
                    "head_branch": "feature/scoped-guardrail",
                    "base_branch": "main",
                    "linked_at": "2026-01-01T00:00:00+00:00",
                }
            }
        }
        repo_a = SimpleNamespace(
            id="repo-a",
            path="/tmp/repo-a",
            scripts={
                GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme", "repo": "repo-a"}),
                GITHUB_TASK_PR_MAPPING_KEY: json.dumps(pr_mapping),
            },
        )
        repo_b = SimpleNamespace(
            id="repo-b",
            path="/tmp/repo-b",
            scripts={GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme", "repo": "repo-b"})},
        )
        ctx.project_service.get_project_repos = AsyncMock(return_value=[repo_a, repo_b])

        reviewed = await api.request_review(task.id)

        assert reviewed is not None
        assert reviewed.status == TaskStatus.REVIEW

    async def test_request_review_fails_closed_when_guardrails_cannot_be_checked(
        self,
        handle_env: tuple,
    ) -> None:
        """Guardrail backend failures should block REVIEW transition with explicit error."""
        _repo, api, ctx = handle_env
        task = await api.create_task("Guardrail Failure Task")
        ctx.project_service.get_project_repos = AsyncMock(
            side_effect=RuntimeError("db unavailable")
        )

        with pytest.raises(ValueError, match="failed to verify GitHub guardrails"):
            await api.request_review(task.id)

    async def test_request_review_skips_lease_check_when_repo_disables_enforcement(
        self,
        handle_env: tuple,
    ) -> None:
        """Lease guardrail checks must be skipped when repo opts out explicitly."""
        _repo, api, ctx = handle_env
        task = await api.create_task("Lease opt-out")
        pr_mapping = {
            "task_to_pr": {
                task.id: {
                    "pr_number": 7,
                    "pr_url": "https://github.com/acme/repo-a/pull/7",
                    "pr_state": "OPEN",
                    "head_branch": "feature/lease-opt-out",
                    "base_branch": "main",
                    "linked_at": "2026-01-01T00:00:00+00:00",
                }
            }
        }
        issue_mapping = {
            "issue_to_task": {"123": task.id},
            "task_to_issue": {task.id: 123},
        }
        repo_a = SimpleNamespace(
            id="repo-a",
            path="/tmp/repo-a",
            scripts={
                GITHUB_CONNECTION_KEY: json.dumps({"owner": "acme", "repo": "repo-a"}),
                GITHUB_TASK_PR_MAPPING_KEY: json.dumps(pr_mapping),
                GITHUB_ISSUE_MAPPING_KEY: json.dumps(issue_mapping),
                GITHUB_LEASE_ENFORCEMENT_KEY: "false",
            },
        )
        ctx.project_service.get_project_repos = AsyncMock(return_value=[repo_a])

        with patch(
            "kagan.core.plugins.github.gh_adapter.resolve_gh_cli",
            side_effect=RuntimeError("should not resolve gh when lease enforcement is disabled"),
        ):
            reviewed = await api.request_review(task.id)

        assert reviewed is not None
        assert reviewed.status == TaskStatus.REVIEW

    async def test_reject_task(self, handle_env: tuple) -> None:
        _repo, api, ctx = handle_env
        task = await api.create_task("Reject Task")
        await api.request_review(task.id)
        refreshed = await api.get_task(task.id)
        ctx.merge_service.apply_rejection_feedback = AsyncMock(return_value=refreshed)
        result = await api.reject_task(task.id, feedback="Needs work")
        assert result is not None
        ctx.merge_service.apply_rejection_feedback.assert_called_once()

    async def test_reject_task_not_found(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        result = await api.reject_task("nonexistent-id")
        assert result is None

    async def test_merge_task(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task("Merge Task")
        success, message = await api.merge_task(task.id)
        assert success is True
        assert "Merged" in message

    async def test_merge_task_not_found(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        success, message = await api.merge_task("nonexistent-id")
        assert success is False
        assert "not found" in message


# ── Job Group ─────────────────────────────────────────────────────────


class TestJobOperations:
    """Tests for job submission and lifecycle."""

    async def test_submit_job(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task("Job Task")
        record = await api.submit_job(task.id, "start_agent")
        assert record.job_id == "job-1"
        assert record.action == "start_agent"

    async def test_get_job(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        record = await api.get_job("job-1")
        assert record is not None
        assert record.job_id == "job-1"

    async def test_cancel_job(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        record = await api.cancel_job("job-1", task_id="task-1")
        assert record is not None

    async def test_wait_job(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        record = await api.wait_job("job-1", task_id="task-1", timeout_seconds=0.1)
        assert record is not None

    async def test_get_job_events(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        events = await api.get_job_events("job-1", task_id="task-1")
        assert events is not None
        assert isinstance(events, list)


# ── Session Group ─────────────────────────────────────────────────────


class TestSessionOperations:
    """Tests for PAIR session lifecycle."""

    async def test_create_session(self, handle_env: tuple) -> None:
        _repo, api, ctx = handle_env
        task = await api.create_task("Session Task")
        await api.update_task(task.id, task_type=TaskType.PAIR)
        ctx.workspace_service.get_path = AsyncMock(return_value="/tmp/worktree")
        result = await api.create_session(task.id)
        assert result.session_name == "kagan-test-session"
        assert result.already_exists is False

    async def test_create_session_task_not_found(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        with pytest.raises(ValueError, match="not found"):
            await api.create_session("nonexistent-id")

    async def test_create_session_non_pair_task(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        task = await api.create_task("Auto Task")
        await api.update_task(task.id, task_type=TaskType.AUTO)
        with pytest.raises(ValueError, match="PAIR"):
            await api.create_session(task.id)

    async def test_session_exists(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        exists = await api.session_exists("some-task-id")
        assert exists is False

    async def test_kill_session(self, handle_env: tuple) -> None:
        _repo, api, ctx = handle_env
        await api.kill_session("some-task-id")
        ctx.session_service.kill_session.assert_called_once_with("some-task-id")

    async def test_attach_session(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        attached = await api.attach_session("some-task-id")
        assert attached is True


# ── Project Group ─────────────────────────────────────────────────────


class TestProjectOperations:
    """Tests for project CRUD operations."""

    async def test_create_and_open_project(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        project_id = await api.create_project("New Project")
        assert isinstance(project_id, str)
        assert len(project_id) > 0
        project = await api.open_project(project_id)
        assert project.name == "New Project"

    async def test_get_project(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        project_id = await api.create_project("Get Project")
        project = await api.get_project(project_id)
        assert project is not None
        assert project.name == "Get Project"

    async def test_get_project_not_found(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        project = await api.get_project("nonexistent-id")
        assert project is None

    async def test_list_projects(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        await api.create_project("Project Alpha")
        await api.create_project("Project Beta")
        projects = await api.list_projects()
        names = {p.name for p in projects}
        assert "Project Alpha" in names
        assert "Project Beta" in names

    async def test_add_repo_and_get_repos(self, handle_env: tuple, tmp_path: Path) -> None:
        _repo, api, _ctx = handle_env
        project_id = await api.create_project("Repo Project")
        repo_path = tmp_path / "my-repo"
        repo_path.mkdir()
        repo_id = await api.add_repo(project_id, repo_path)
        assert isinstance(repo_id, str)
        repos = await api.get_project_repos(project_id)
        assert len(repos) >= 1
        paths = [r.path for r in repos]
        assert str(repo_path.resolve()) in paths

    async def test_get_project_repo_details(self, handle_env: tuple, tmp_path: Path) -> None:
        _repo, api, _ctx = handle_env
        project_id = await api.create_project("Details Project")
        repo_path = tmp_path / "detail-repo"
        repo_path.mkdir()
        await api.add_repo(project_id, repo_path, is_primary=True)
        details = await api.get_project_repo_details(project_id)
        assert len(details) >= 1
        assert details[0]["is_primary"] is True

    async def test_find_project_by_repo_path(self, handle_env: tuple, tmp_path: Path) -> None:
        _repo, api, _ctx = handle_env
        repo_path = tmp_path / "find-repo"
        repo_path.mkdir()
        project_id = await api.create_project("Find Project", repo_paths=[repo_path])
        found = await api.find_project_by_repo_path(repo_path)
        assert found is not None
        assert found.id == project_id

    async def test_find_project_not_found(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        found = await api.find_project_by_repo_path("/nonexistent/path")
        assert found is None


# ── Settings & Audit Group ────────────────────────────────────────────


class TestSettingsAndAudit:
    """Tests for settings and audit operations."""

    async def test_get_settings(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        settings = await api.get_settings()
        assert "general.default_base_branch" not in settings
        assert settings["general.worktree_base_ref_strategy"] == "remote"

    async def test_update_settings(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        success, _msg, updates = await api.update_settings({"general.auto_review": True})
        assert success is True
        assert updates["general.auto_review"] is True
        settings = await api.get_settings()
        assert settings["general.auto_review"] is True

    async def test_update_settings_worktree_base_ref_strategy(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        success, _msg, updates = await api.update_settings(
            {"general.worktree_base_ref_strategy": "local_if_ahead"}
        )
        assert success is True
        assert updates["general.worktree_base_ref_strategy"] == "local_if_ahead"
        settings = await api.get_settings()
        assert settings["general.worktree_base_ref_strategy"] == "local_if_ahead"

    async def test_update_settings_rejects_invalid_worktree_base_ref_strategy(
        self,
        handle_env: tuple,
    ) -> None:
        _repo, api, _ctx = handle_env
        success, message, _updates = await api.update_settings(
            {"general.worktree_base_ref_strategy": "invalid"}
        )
        assert success is False
        assert "must be one of" in message

    async def test_update_settings_empty(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        success, _msg, _updates = await api.update_settings({})
        assert success is False

    async def test_update_settings_invalid_field(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        success, message, _updates = await api.update_settings({"nonexistent.field": True})
        assert success is False
        assert "Unsupported" in message

    async def test_list_audit_events(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        events = await api.list_audit_events()
        assert isinstance(events, list)

    async def test_get_instrumentation(self, handle_env: tuple) -> None:
        _repo, api, _ctx = handle_env
        data = await api.get_instrumentation()
        assert isinstance(data, dict)


# ── Orchestration Tests ───────────────────────────────────────────────


class TestOrchestration:
    """Tests that verify multi-service coordination in api methods."""

    async def test_delete_task_coordinates_services(self, handle_env: tuple) -> None:
        """delete_task delegates to merge_service.delete_task for full cleanup."""
        _repo, api, ctx = handle_env
        task = await api.create_task("Orchestrated Delete")
        success, _msg = await api.delete_task(task.id)
        assert success is True
        ctx.merge_service.delete_task.assert_called_once()

    async def test_update_task_type_pair_to_auto(self, handle_env: tuple) -> None:
        """Switching from PAIR to AUTO kills the session."""
        _repo, api, ctx = handle_env
        task = await api.create_task("Type Transition")
        assert task.task_type == TaskType.PAIR
        ctx.session_service.session_exists = AsyncMock(return_value=True)
        updated = await api.update_task(task.id, task_type=TaskType.AUTO)
        assert updated is not None
        assert updated.task_type == TaskType.AUTO
        ctx.session_service.kill_session.assert_called_once_with(task.id)

    async def test_update_task_type_auto_to_pair(self, handle_env: tuple) -> None:
        """Switching from AUTO to PAIR stops the automation agent."""
        _repo, api, ctx = handle_env
        task = await api.create_task("Type Transition 2")
        await api.update_task(task.id, task_type=TaskType.AUTO)
        ctx.automation_service.is_running = MagicMock(return_value=True)
        updated = await api.update_task(task.id, task_type=TaskType.PAIR)
        assert updated is not None
        assert updated.task_type == TaskType.PAIR
        ctx.automation_service.stop_task.assert_called_once_with(task.id)

    async def test_scratchpad_appends_content(self, handle_env: tuple) -> None:
        """update_scratchpad appends, not replaces."""
        _repo, api, _ctx = handle_env
        task = await api.create_task("Scratchpad Append")
        await api.update_scratchpad(task.id, "First note")
        await api.update_scratchpad(task.id, "Second note")
        content = await api.get_scratchpad(task.id)
        assert "First note" in content
        assert "Second note" in content

    async def test_ctx_property_returns_app_context(self, handle_env: tuple) -> None:
        """The ctx property exposes the underlying AppContext."""
        _repo, api, ctx = handle_env
        assert api.ctx is ctx

    async def test_create_session_reuse_existing(self, handle_env: tuple) -> None:
        """When session already exists and reuse_if_exists=True, returns name."""
        _repo, api, ctx = handle_env
        task = await api.create_task("Reuse Session")
        ctx.workspace_service.get_path = AsyncMock(return_value="/tmp/worktree")
        ctx.session_service.session_exists = AsyncMock(return_value=True)
        result = await api.create_session(task.id)
        assert result.session_name == f"kagan-{task.id}"
        assert result.already_exists is True
        ctx.session_service.create_session.assert_not_called()
