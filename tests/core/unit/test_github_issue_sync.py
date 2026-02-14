"""Tests for GitHub issue sync, mapping, and mode resolution."""

from __future__ import annotations

import json
from typing import Any
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from kagan.core.models.enums import TaskStatus, TaskType
from kagan.core.plugins.github.domain.repo_state import (
    encode_lease_enforcement_update,
    load_lease_enforcement_state,
)
from kagan.core.plugins.github.entrypoints.plugin_handlers import GH_NOT_CONNECTED
from kagan.core.plugins.github.gh_adapter import (
    GITHUB_CONNECTION_KEY,
    GhIssue,
    parse_gh_issue_list,
)
from kagan.core.plugins.github.sync import (
    GITHUB_DEFAULT_MODE_KEY,
    GITHUB_ISSUE_MAPPING_KEY,
    GITHUB_LEASE_ENFORCEMENT_KEY,
    IssueMapping,
    SyncCheckpoint,
    build_task_title_from_issue,
    compute_issue_changes,
    filter_issues_since_checkpoint,
    load_lease_enforcement,
    load_repo_default_mode,
    resolve_task_status_from_issue_state,
    resolve_task_type_from_labels,
)


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


class TestRepoDefaultMode:
    """Tests for repo default mode behavior used by sync outcome policy."""

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


class TestRepoLeaseEnforcement:
    """Tests for repo lease enforcement policy parsing and typed adapter behavior."""

    def test_load_lease_enforcement_defaults_to_true(self) -> None:
        assert load_lease_enforcement(None) is True
        assert load_lease_enforcement({}) is True

    @pytest.mark.parametrize("raw_value", ["false", "FALSE", " false ", False])
    def test_load_lease_enforcement_parses_explicit_opt_out_values(self, raw_value: object) -> None:
        scripts = {GITHUB_LEASE_ENFORCEMENT_KEY: raw_value}
        assert load_lease_enforcement(scripts) is False

    @pytest.mark.parametrize("raw_value", ["true", "TRUE", " true ", True])
    def test_load_lease_enforcement_parses_explicit_opt_in_values(self, raw_value: object) -> None:
        scripts = {GITHUB_LEASE_ENFORCEMENT_KEY: raw_value}
        assert load_lease_enforcement(scripts) is True

    @pytest.mark.parametrize("raw_value", ["0", "off", "disabled", "typo", 0, 1])
    def test_load_lease_enforcement_invalid_or_legacy_values_default_to_true_with_warning(
        self,
        raw_value: object,
        caplog: pytest.LogCaptureFixture,
    ) -> None:
        scripts = {GITHUB_LEASE_ENFORCEMENT_KEY: raw_value}
        assert load_lease_enforcement(scripts) is True
        assert "expected true/false" in caplog.text

    def test_typed_adapter_round_trip_for_lease_enforcement(self) -> None:
        scripts = encode_lease_enforcement_update(False)
        assert load_lease_enforcement_state(scripts) is False


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

    def test_filter_issues_since_checkpoint_handles_naive_issue_timestamps(self) -> None:
        checkpoint = SyncCheckpoint(last_sync_at="2025-01-02T00:00:00Z", issue_count=2)
        issues = [
            GhIssue(
                number=1,
                title="old",
                state="OPEN",
                labels=[],
                updated_at="2025-01-01T00:00:00",
            ),
            GhIssue(
                number=2,
                title="new",
                state="OPEN",
                labels=[],
                updated_at="2025-01-03T00:00:00",
            ),
        ]

        filtered = filter_issues_since_checkpoint(issues, checkpoint)

        assert [issue.number for issue in filtered] == [2]


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


class TestSyncIssuesHandler:
    """Tests for sync_issues handler logic."""

    @pytest.mark.asyncio()
    async def test_returns_error_when_repo_not_connected(self) -> None:
        from kagan.core.plugins.github.entrypoints.plugin_handlers import handle_sync_issues

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
        from kagan.core.plugins.github.entrypoints.plugin_handlers import handle_sync_issues

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
                GITHUB_CONNECTION_KEY: json.dumps({"host": "github.com", "repo": "repo-1"}),
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
                "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.resolve_gh_cli_path",
                return_value=("/usr/bin/gh", None),
            ),
            patch(
                "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.run_gh_issue_list",
                return_value=(mock_issues, None),
            ),
            patch(
                "kagan.core.plugins.github.adapters.core_gateway.AppContextCoreGateway.update_repo_scripts",
                new_callable=AsyncMock,
            ),
        ):
            result = await handle_sync_issues(ctx, params)

        assert result["success"] is True
        assert result["stats"]["no_change"] == 1
        assert result["stats"]["inserted"] == 0
        assert result["stats"]["updated"] == 0

    @pytest.mark.asyncio()
    async def test_sync_recreates_mapping_without_stale_reverse_entry(self) -> None:
        """Drift recovery should replace stale task_to_issue entries for recreated tasks."""
        from kagan.core.plugins.github.entrypoints.plugin_handlers import handle_sync_issues

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
                GITHUB_CONNECTION_KEY: json.dumps({"host": "github.com", "repo": "repo-1"}),
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
                "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.resolve_gh_cli_path",
                return_value=("/usr/bin/gh", None),
            ),
            patch(
                "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.run_gh_issue_list",
                return_value=(mock_issues, None),
            ),
            patch(
                "kagan.core.plugins.github.adapters.core_gateway.AppContextCoreGateway.update_repo_scripts",
                new_callable=AsyncMock,
            ) as update_repo_scripts,
        ):
            result = await handle_sync_issues(ctx, {"project_id": "project-1"})

        assert result["success"] is True
        updates = update_repo_scripts.await_args.args[-1]
        mapping_payload = json.loads(updates[GITHUB_ISSUE_MAPPING_KEY])
        assert mapping_payload["issue_to_task"] == {"1": "task-new"}
        assert mapping_payload["task_to_issue"] == {"task-new": 1}

    @pytest.mark.asyncio()
    async def test_sync_returns_failure_when_issue_projection_errors_occur(self) -> None:
        """Per-issue projection errors should fail sync result while preserving stats."""
        from kagan.core.plugins.github.entrypoints.plugin_handlers import handle_sync_issues

        ctx = MagicMock()

        async def get_project_async(project_id: str) -> MagicMock:
            return MagicMock(id=project_id)

        async def get_repos_async(project_id: str) -> list:
            repo = MagicMock()
            repo.id = "repo-1"
            repo.path = "/tmp/repo"
            repo.scripts = {
                GITHUB_CONNECTION_KEY: json.dumps({"host": "github.com", "repo": "repo-1"}),
            }
            return [repo]

        async def create_task_async(*_args: Any, **_kwargs: Any) -> MagicMock:
            raise RuntimeError("simulated create error")

        ctx.project_service.get_project = get_project_async
        ctx.project_service.get_project_repos = get_repos_async
        ctx.task_service.get_task = AsyncMock(return_value=None)
        ctx.task_service.create_task = create_task_async
        ctx.task_service.update_fields = AsyncMock(return_value=None)

        mock_issues = [
            {
                "number": 1,
                "title": "Failing issue projection",
                "state": "OPEN",
                "labels": [],
                "updatedAt": "2025-01-10T00:00:00Z",
            }
        ]

        with (
            patch(
                "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.resolve_gh_cli_path",
                return_value=("/usr/bin/gh", None),
            ),
            patch(
                "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.run_gh_issue_list",
                return_value=(mock_issues, None),
            ),
            patch(
                "kagan.core.plugins.github.adapters.core_gateway.AppContextCoreGateway.update_repo_scripts",
                new_callable=AsyncMock,
            ),
        ):
            result = await handle_sync_issues(ctx, {"project_id": "project-1"})

        assert result["success"] is False
        assert result["code"] == "GH_SYNC_FAILED"
        assert "per-issue errors" in result["message"]
        assert "retry sync" in result["hint"]
        assert result["stats"]["total"] == 1
        assert result["stats"]["errors"] == 1
        assert result["stats"]["inserted"] == 0
        assert result["stats"]["updated"] == 0
        assert result["stats"]["reopened"] == 0
        assert result["stats"]["closed"] == 0
        assert result["stats"]["no_change"] == 0


class TestLeaseEnforcementOptOutHandler:
    """Tests for repo-level lease enforcement opt-out behavior."""

    @staticmethod
    def _build_connected_repo_scripts_with_opt_out() -> dict[str, str]:
        return {
            GITHUB_CONNECTION_KEY: json.dumps({"host": "github.com", "repo": "repo-1"}),
            GITHUB_LEASE_ENFORCEMENT_KEY: "false",
        }

    @pytest.mark.asyncio()
    async def test_acquire_lease_skips_gh_calls_when_enforcement_disabled(self) -> None:
        from kagan.core.plugins.github.entrypoints.plugin_handlers import handle_acquire_lease

        ctx = MagicMock()

        async def get_project_async(project_id: str) -> MagicMock:
            return MagicMock(id=project_id)

        async def get_repos_async(project_id: str) -> list:
            repo = MagicMock()
            repo.id = "repo-1"
            repo.path = "/tmp/repo"
            repo.scripts = self._build_connected_repo_scripts_with_opt_out()
            return [repo]

        ctx.project_service.get_project = get_project_async
        ctx.project_service.get_project_repos = get_repos_async

        with (
            patch(
                "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.resolve_gh_cli_path"
            ) as resolve_gh_cli_path,
            patch(
                "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.acquire_lease"
            ) as acquire_lease,
        ):
            result = await handle_acquire_lease(
                ctx,
                {"project_id": "project-1", "issue_number": 42},
            )

        assert result["success"] is True
        assert result["code"] == "LEASE_ENFORCEMENT_DISABLED"
        assert "skipping acquire" in result["message"].lower()
        resolve_gh_cli_path.assert_not_called()
        acquire_lease.assert_not_called()

    @pytest.mark.asyncio()
    async def test_release_lease_skips_gh_calls_when_enforcement_disabled(self) -> None:
        from kagan.core.plugins.github.entrypoints.plugin_handlers import handle_release_lease

        ctx = MagicMock()

        async def get_project_async(project_id: str) -> MagicMock:
            return MagicMock(id=project_id)

        async def get_repos_async(project_id: str) -> list:
            repo = MagicMock()
            repo.id = "repo-1"
            repo.path = "/tmp/repo"
            repo.scripts = self._build_connected_repo_scripts_with_opt_out()
            return [repo]

        ctx.project_service.get_project = get_project_async
        ctx.project_service.get_project_repos = get_repos_async

        with (
            patch(
                "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.resolve_gh_cli_path"
            ) as resolve_gh_cli_path,
            patch(
                "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.release_lease"
            ) as release_lease,
        ):
            result = await handle_release_lease(
                ctx,
                {"project_id": "project-1", "issue_number": 42},
            )

        assert result["success"] is True
        assert result["code"] == "LEASE_ENFORCEMENT_DISABLED"
        assert "skipping release" in result["message"].lower()
        resolve_gh_cli_path.assert_not_called()
        release_lease.assert_not_called()

    @pytest.mark.asyncio()
    async def test_get_lease_state_returns_unlocked_when_enforcement_disabled(self) -> None:
        from kagan.core.plugins.github.entrypoints.plugin_handlers import handle_get_lease_state

        ctx = MagicMock()

        async def get_project_async(project_id: str) -> MagicMock:
            return MagicMock(id=project_id)

        async def get_repos_async(project_id: str) -> list:
            repo = MagicMock()
            repo.id = "repo-1"
            repo.path = "/tmp/repo"
            repo.scripts = self._build_connected_repo_scripts_with_opt_out()
            return [repo]

        ctx.project_service.get_project = get_project_async
        ctx.project_service.get_project_repos = get_repos_async

        with (
            patch(
                "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.resolve_gh_cli_path"
            ) as resolve_gh_cli_path,
            patch(
                "kagan.core.plugins.github.adapters.gh_cli_client.GhCliClientAdapter.get_lease_state"
            ) as get_lease_state,
        ):
            result = await handle_get_lease_state(
                ctx,
                {"project_id": "project-1", "issue_number": 42},
            )

        assert result["success"] is True
        assert result["code"] == "LEASE_STATE_OK"
        assert result["state"] == {
            "is_locked": False,
            "is_held_by_current_instance": False,
            "can_acquire": True,
            "requires_takeover": False,
            "holder": None,
            "enforcement_enabled": False,
        }
        resolve_gh_cli_path.assert_not_called()
        get_lease_state.assert_not_called()
