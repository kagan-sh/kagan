"""Tests for GitHub issue sync, mapping, and mode resolution."""

from __future__ import annotations

import json
from typing import Any
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from kagan.core.models.enums import TaskStatus, TaskType
from kagan.core.plugins.github import (
    GITHUB_CAPABILITY,
    register_github_plugin,
)
from kagan.core.plugins.github.contract import GITHUB_METHOD_SYNC_ISSUES
from kagan.core.plugins.github.gh_adapter import (
    GITHUB_CONNECTION_KEY,
    GhIssue,
    parse_gh_issue_list,
)
from kagan.core.plugins.github.runtime import GH_NOT_CONNECTED
from kagan.core.plugins.github.sync import (
    GITHUB_DEFAULT_MODE_KEY,
    GITHUB_ISSUE_MAPPING_KEY,
    IssueMapping,
    SyncCheckpoint,
    build_task_title_from_issue,
    compute_issue_changes,
    filter_issues_since_checkpoint,
    load_checkpoint,
    load_mapping,
    load_repo_default_mode,
    resolve_task_status_from_issue_state,
    resolve_task_type_from_labels,
)
from kagan.core.plugins.sdk import PluginRegistry


class TestModeResolution:
    """Tests for deterministic task type resolution from labels."""

    def test_auto_label_resolves_to_auto_type(self) -> None:
        labels = ["bug", "kagan:mode:auto", "enhancement"]
        result = resolve_task_type_from_labels(labels)
        assert result.task_type == TaskType.AUTO
        assert result.source == "label"
        assert result.conflict is False

    def test_pair_label_resolves_to_pair_type(self) -> None:
        labels = ["feature", "kagan:mode:pair"]
        result = resolve_task_type_from_labels(labels)
        assert result.task_type == TaskType.PAIR
        assert result.source == "label"
        assert result.conflict is False

    def test_conflicting_labels_resolve_to_pair_with_warning(
        self, caplog: pytest.LogCaptureFixture
    ) -> None:
        """When both mode labels are present, PAIR wins deterministically with warning."""
        labels = ["kagan:mode:pair", "kagan:mode:auto"]
        result = resolve_task_type_from_labels(labels)
        assert result.task_type == TaskType.PAIR
        assert result.source == "label"
        assert result.conflict is True
        assert "Conflicting mode labels" in caplog.text

    def test_no_mode_label_uses_v1_default(self) -> None:
        labels = ["bug", "high-priority"]
        result = resolve_task_type_from_labels(labels)
        assert result.task_type == TaskType.PAIR  # V1 default
        assert result.source == "v1_default"
        assert result.conflict is False

    def test_no_mode_label_uses_repo_default_when_configured(self) -> None:
        labels = ["bug"]
        result = resolve_task_type_from_labels(labels, repo_default=TaskType.AUTO)
        assert result.task_type == TaskType.AUTO
        assert result.source == "repo_default"
        assert result.conflict is False

    def test_label_takes_precedence_over_repo_default(self) -> None:
        labels = ["kagan:mode:pair"]
        result = resolve_task_type_from_labels(labels, repo_default=TaskType.AUTO)
        assert result.task_type == TaskType.PAIR
        assert result.source == "label"
        assert result.conflict is False

    def test_case_insensitive_label_matching(self) -> None:
        labels = ["KAGAN:MODE:AUTO"]
        result = resolve_task_type_from_labels(labels)
        assert result.task_type == TaskType.AUTO
        assert result.source == "label"


class TestTaskStatusResolution:
    """Tests for issue state to task status mapping."""

    def test_open_issue_maps_to_backlog(self) -> None:
        assert resolve_task_status_from_issue_state("OPEN") == TaskStatus.BACKLOG

    def test_closed_issue_maps_to_done(self) -> None:
        assert resolve_task_status_from_issue_state("CLOSED") == TaskStatus.DONE

    def test_case_insensitive_state_matching(self) -> None:
        assert resolve_task_status_from_issue_state("open") == TaskStatus.BACKLOG
        assert resolve_task_status_from_issue_state("Closed") == TaskStatus.DONE


class TestTaskTitleFormat:
    """Tests for task title formatting from issue."""

    def test_title_includes_gh_prefix_and_number(self) -> None:
        result = build_task_title_from_issue(42, "Fix login bug")
        assert result == "[GH-42] Fix login bug"

    def test_title_preserves_issue_title(self) -> None:
        result = build_task_title_from_issue(1, "Complex: title with [brackets]")
        assert result == "[GH-1] Complex: title with [brackets]"


class TestIssueMapping:
    """Tests for bidirectional issue-to-task mapping."""

    def test_add_mapping_creates_bidirectional_link(self) -> None:
        mapping = IssueMapping()
        mapping.add_mapping(42, "task-abc")

        assert mapping.get_task_id(42) == "task-abc"
        assert mapping.get_issue_number("task-abc") == 42

    def test_remove_by_issue_clears_both_directions(self) -> None:
        mapping = IssueMapping()
        mapping.add_mapping(42, "task-abc")
        mapping.remove_by_issue(42)

        assert mapping.get_task_id(42) is None
        assert mapping.get_issue_number("task-abc") is None

    def test_remove_by_task_clears_both_directions(self) -> None:
        mapping = IssueMapping()
        mapping.add_mapping(42, "task-abc")
        mapping.remove_by_task("task-abc")

        assert mapping.get_task_id(42) is None
        assert mapping.get_issue_number("task-abc") is None

    def test_serialization_round_trip(self) -> None:
        mapping = IssueMapping()
        mapping.add_mapping(1, "task-a")
        mapping.add_mapping(2, "task-b")

        data = mapping.to_dict()
        restored = IssueMapping.from_dict(data)

        assert restored.get_task_id(1) == "task-a"
        assert restored.get_task_id(2) == "task-b"
        assert restored.get_issue_number("task-a") == 1


class TestSyncCheckpoint:
    """Tests for sync checkpoint persistence."""

    def test_serialization_round_trip(self) -> None:
        checkpoint = SyncCheckpoint(last_sync_at="2025-01-01T00:00:00Z", issue_count=10)
        data = checkpoint.to_dict()
        restored = SyncCheckpoint.from_dict(data)

        assert restored.last_sync_at == "2025-01-01T00:00:00Z"
        assert restored.issue_count == 10

    def test_from_dict_handles_none(self) -> None:
        restored = SyncCheckpoint.from_dict(None)
        assert restored.last_sync_at is None
        assert restored.issue_count == 0


class TestLoadHelpers:
    """Tests for loading checkpoint and mapping from scripts."""

    def test_load_checkpoint_parses_json_and_defaults(self) -> None:
        scripts = {
            "kagan.github.sync_checkpoint": json.dumps(
                {"last_sync_at": "2025-01-01T00:00:00Z"}
            )
        }
        checkpoint = load_checkpoint(scripts)
        assert checkpoint.last_sync_at == "2025-01-01T00:00:00Z"
        assert checkpoint.issue_count == 0

    def test_load_checkpoint_returns_empty_when_missing(self) -> None:
        checkpoint = load_checkpoint({})
        assert checkpoint.last_sync_at is None
        assert checkpoint.issue_count == 0

    def test_load_mapping_from_json_string(self) -> None:
        scripts = {
            "kagan.github.issue_mapping": json.dumps(
                {"issue_to_task": {"1": "task-a"}, "task_to_issue": {"task-a": 1}}
            )
        }
        mapping = load_mapping(scripts)
        assert mapping.get_task_id(1) == "task-a"

    @pytest.mark.parametrize(
        ("raw_mode", "expected"),
        [
            ("AUTO", TaskType.AUTO),
            ("auto", TaskType.AUTO),
            ("PAIR", TaskType.PAIR),
            ("invalid", None),
            (None, None),
        ],
    )
    def test_load_repo_default_mode_normalization(
        self,
        raw_mode: str | None,
        expected: TaskType | None,
    ) -> None:
        scripts = {GITHUB_DEFAULT_MODE_KEY: raw_mode} if raw_mode is not None else {}
        assert load_repo_default_mode(scripts) == expected

    def test_load_repo_default_mode_none_scripts(self) -> None:
        assert load_repo_default_mode(None) is None


class TestIncrementalIssueFiltering:
    """Tests for checkpoint-aware incremental issue filtering."""

    def test_filter_issues_since_checkpoint_uses_updated_at(self) -> None:
        checkpoint = SyncCheckpoint(last_sync_at="2025-01-02T00:00:00Z", issue_count=2)
        issues = [
            GhIssue(
                number=1,
                title="old",
                state="OPEN",
                labels=[],
                updated_at="2025-01-01T00:00:00Z",
            ),
            GhIssue(
                number=2,
                title="new",
                state="OPEN",
                labels=[],
                updated_at="2025-01-03T00:00:00Z",
            ),
        ]

        filtered = filter_issues_since_checkpoint(issues, checkpoint)

        assert [issue.number for issue in filtered] == [2]

    def test_filter_issues_since_checkpoint_keeps_entries_with_invalid_timestamps(self) -> None:
        checkpoint = SyncCheckpoint(last_sync_at="2025-01-02T00:00:00Z", issue_count=1)
        issues = [GhIssue(number=5, title="missing-ts", state="OPEN", labels=[], updated_at="")]

        filtered = filter_issues_since_checkpoint(issues, checkpoint)

        assert [issue.number for issue in filtered] == [5]


class TestComputeIssueChanges:
    """Tests for computing sync actions from issue state."""

    def test_new_issue_returns_insert_action(self) -> None:
        issue = GhIssue(number=1, title="New feature", state="OPEN", labels=[], updated_at="")
        mapping = IssueMapping()
        existing_tasks: dict[str, Any] = {}

        action, changes = compute_issue_changes(issue, mapping, existing_tasks)

        assert action == "insert"
        assert changes is not None
        assert changes["title"] == "[GH-1] New feature"
        assert changes["status"] == TaskStatus.BACKLOG

    def test_existing_unchanged_issue_returns_no_change(self) -> None:
        issue = GhIssue(number=1, title="Feature", state="OPEN", labels=[], updated_at="")
        mapping = IssueMapping()
        mapping.add_mapping(1, "task-a")
        existing_tasks = {
            "task-a": {
                "title": "[GH-1] Feature",
                "status": TaskStatus.BACKLOG,
                "task_type": TaskType.PAIR,
            }
        }

        action, changes = compute_issue_changes(issue, mapping, existing_tasks)

        assert action == "no_change"
        assert changes is None

    def test_closed_issue_returns_close_action(self) -> None:
        issue = GhIssue(number=1, title="Feature", state="CLOSED", labels=[], updated_at="")
        mapping = IssueMapping()
        mapping.add_mapping(1, "task-a")
        existing_tasks = {
            "task-a": {
                "title": "[GH-1] Feature",
                "status": TaskStatus.BACKLOG,
                "task_type": TaskType.PAIR,
            }
        }

        action, changes = compute_issue_changes(issue, mapping, existing_tasks)

        assert action == "close"
        assert changes is not None
        assert changes["status"] == TaskStatus.DONE

    def test_reopened_issue_returns_reopen_action(self) -> None:
        issue = GhIssue(number=1, title="Feature", state="OPEN", labels=[], updated_at="")
        mapping = IssueMapping()
        mapping.add_mapping(1, "task-a")
        existing_tasks = {
            "task-a": {
                "title": "[GH-1] Feature",
                "status": TaskStatus.DONE,
                "task_type": TaskType.PAIR,
            }
        }

        action, changes = compute_issue_changes(issue, mapping, existing_tasks)

        assert action == "reopen"
        assert changes is not None
        assert changes["status"] == TaskStatus.BACKLOG

    def test_title_change_returns_update_action(self) -> None:
        issue = GhIssue(number=1, title="Updated title", state="OPEN", labels=[], updated_at="")
        mapping = IssueMapping()
        mapping.add_mapping(1, "task-a")
        existing_tasks = {
            "task-a": {
                "title": "[GH-1] Old title",
                "status": TaskStatus.BACKLOG,
                "task_type": TaskType.PAIR,
            }
        }

        action, changes = compute_issue_changes(issue, mapping, existing_tasks)

        assert action == "update"
        assert changes is not None
        assert changes["title"] == "[GH-1] Updated title"

    def test_mode_label_change_returns_update_action(self) -> None:
        issue = GhIssue(
            number=1, title="Feature", state="OPEN", labels=["kagan:mode:auto"], updated_at=""
        )
        mapping = IssueMapping()
        mapping.add_mapping(1, "task-a")
        existing_tasks = {
            "task-a": {
                "title": "[GH-1] Feature",
                "status": TaskStatus.BACKLOG,
                "task_type": TaskType.PAIR,
            }
        }

        action, changes = compute_issue_changes(issue, mapping, existing_tasks)

        assert action == "update"
        assert changes is not None
        assert changes["task_type"] == TaskType.AUTO

    def test_missing_task_triggers_drift_recovery_insert(self) -> None:
        issue = GhIssue(number=1, title="Feature", state="OPEN", labels=[], updated_at="")
        mapping = IssueMapping()
        mapping.add_mapping(1, "task-deleted")
        existing_tasks: dict[str, Any] = {}  # Task was deleted

        action, changes = compute_issue_changes(issue, mapping, existing_tasks)

        assert action == "insert"
        assert changes is not None
        assert changes["title"] == "[GH-1] Feature"


class TestParseGhIssueList:
    """Tests for parsing gh issue list JSON output."""

    def test_parses_valid_issue_list(self) -> None:
        raw = [
            {
                "number": 1,
                "title": "Bug fix",
                "state": "OPEN",
                "labels": [{"name": "bug"}],
                "updatedAt": "2025-01-01T00:00:00Z",
            },
            {
                "number": 2,
                "title": "Feature",
                "state": "CLOSED",
                "labels": [],
                "updatedAt": "2025-01-02T00:00:00Z",
            },
        ]

        issues = parse_gh_issue_list(raw)

        assert len(issues) == 2
        assert issues[0].number == 1
        assert issues[0].title == "Bug fix"
        assert issues[0].state == "OPEN"
        assert issues[0].labels == ["bug"]
        assert issues[1].number == 2
        assert issues[1].state == "CLOSED"

    def test_skips_entries_without_number(self) -> None:
        raw = [
            {"title": "No number", "state": "OPEN"},
            {"number": 1, "title": "Valid", "state": "OPEN"},
        ]

        issues = parse_gh_issue_list(raw)

        assert len(issues) == 1
        assert issues[0].number == 1


class TestSyncIssuesRegistration:
    """Tests for sync_issues operation registration."""

    def test_sync_issues_operation_is_registered(self) -> None:
        registry = PluginRegistry()
        register_github_plugin(registry)

        operation = registry.resolve_operation(GITHUB_CAPABILITY, GITHUB_METHOD_SYNC_ISSUES)

        assert operation is not None
        assert operation.mutating is True

    def test_sync_issues_operation_requires_maintainer_profile(self) -> None:
        from kagan.core.security import CapabilityProfile

        registry = PluginRegistry()
        register_github_plugin(registry)

        operation = registry.resolve_operation(GITHUB_CAPABILITY, GITHUB_METHOD_SYNC_ISSUES)

        assert operation is not None
        assert operation.minimum_profile == CapabilityProfile.MAINTAINER


class TestSyncIssuesHandler:
    """Tests for sync_issues handler logic."""

    @pytest.mark.asyncio()
    async def test_returns_error_when_repo_not_connected(self) -> None:
        from kagan.core.plugins.github.runtime import handle_sync_issues

        ctx = MagicMock()

        async def get_project_async(project_id: str) -> MagicMock:
            return MagicMock(id=project_id)

        async def get_repos_async(project_id: str) -> list:
            repo = MagicMock()
            repo.id = "repo-1"
            repo.path = "/tmp/repo"
            repo.scripts = {}  # No connection
            return [repo]

        ctx.project_service.get_project = get_project_async
        ctx.project_service.get_project_repos = get_repos_async
        params = {"project_id": "project-1"}

        result = await handle_sync_issues(ctx, params)

        assert result["success"] is False
        assert result["code"] == GH_NOT_CONNECTED

    @pytest.mark.asyncio()
    async def test_idempotent_sync_produces_no_churn(self) -> None:
        """Re-running sync without remote changes produces no task changes."""
        from kagan.core.plugins.github.runtime import handle_sync_issues

        ctx = MagicMock()

        # Setup: repo is connected, has existing mapping
        existing_mapping = {"issue_to_task": {"1": "task-a"}, "task_to_issue": {"task-a": 1}}

        async def get_project_async(project_id: str) -> MagicMock:
            return MagicMock(id=project_id)

        async def get_repos_async(project_id: str) -> list:
            repo = MagicMock()
            repo.id = "repo-1"
            repo.path = "/tmp/repo"
            repo.scripts = {
                GITHUB_CONNECTION_KEY: json.dumps({"host": "github.com"}),
                GITHUB_ISSUE_MAPPING_KEY: json.dumps(existing_mapping),
            }
            return [repo]

        async def get_task_async(task_id: str) -> MagicMock:
            if task_id == "task-a":
                task = MagicMock()
                task.id = "task-a"
                task.title = "[GH-1] Feature"
                task.status = TaskStatus.BACKLOG
                task.task_type = TaskType.PAIR
                return task
            return None

        ctx.project_service.get_project = get_project_async
        ctx.project_service.get_project_repos = get_repos_async
        ctx.task_service.get_task = get_task_async
        ctx._task_repo = MagicMock()
        ctx._task_repo._session_factory = MagicMock()

        params = {"project_id": "project-1"}

        # Mock gh CLI and issue list
        mock_issues = [
            {
                "number": 1,
                "title": "Feature",
                "state": "OPEN",
                "labels": [],
                "updatedAt": "2025-01-01T00:00:00Z",
            }
        ]

        with (
            patch(
                "kagan.core.plugins.github.runtime.resolve_gh_cli",
                return_value=MagicMock(available=True, path="/usr/bin/gh"),
            ),
            patch(
                "kagan.core.plugins.github.runtime.run_gh_issue_list",
                return_value=(mock_issues, None),
            ),
            patch("kagan.core.plugins.github.runtime._upsert_repo_sync_state"),
        ):
            result = await handle_sync_issues(ctx, params)

        assert result["success"] is True
        assert result["stats"]["no_change"] == 1
        assert result["stats"]["inserted"] == 0
        assert result["stats"]["updated"] == 0

    @pytest.mark.asyncio()
    async def test_sync_recreates_mapping_without_stale_reverse_entry(self) -> None:
        """Drift recovery should replace stale task_to_issue entries for recreated tasks."""
        from kagan.core.plugins.github.runtime import handle_sync_issues

        ctx = MagicMock()
        existing_mapping = {
            "issue_to_task": {"1": "task-deleted"},
            "task_to_issue": {"task-deleted": 1},
        }

        async def get_project_async(project_id: str) -> MagicMock:
            return MagicMock(id=project_id)

        async def get_repos_async(project_id: str) -> list:
            repo = MagicMock()
            repo.id = "repo-1"
            repo.path = "/tmp/repo"
            repo.scripts = {
                GITHUB_CONNECTION_KEY: json.dumps({"host": "github.com"}),
                GITHUB_ISSUE_MAPPING_KEY: json.dumps(existing_mapping),
            }
            return [repo]

        async def get_task_async(task_id: str) -> None:
            return None

        async def create_task_async(*_args: Any, **_kwargs: Any) -> MagicMock:
            task = MagicMock()
            task.id = "task-new"
            return task

        ctx.project_service.get_project = get_project_async
        ctx.project_service.get_project_repos = get_repos_async
        ctx.task_service.get_task = get_task_async
        ctx.task_service.create_task = create_task_async
        ctx.task_service.update_fields = AsyncMock(return_value=None)
        ctx._task_repo = MagicMock()
        ctx._task_repo._session_factory = MagicMock()

        mock_issues = [
            {
                "number": 1,
                "title": "Recreated feature",
                "state": "OPEN",
                "labels": [],
                "updatedAt": "2025-01-05T00:00:00Z",
            }
        ]

        with (
            patch(
                "kagan.core.plugins.github.runtime.resolve_gh_cli",
                return_value=MagicMock(available=True, path="/usr/bin/gh"),
            ),
            patch(
                "kagan.core.plugins.github.runtime.run_gh_issue_list",
                return_value=(mock_issues, None),
            ),
            patch("kagan.core.plugins.github.runtime._upsert_repo_sync_state") as upsert_state,
        ):
            result = await handle_sync_issues(ctx, {"project_id": "project-1"})

        assert result["success"] is True
        mapping = upsert_state.await_args.args[3]
        assert mapping.issue_to_task[1] == "task-new"
        assert mapping.task_to_issue == {"task-new": 1}
