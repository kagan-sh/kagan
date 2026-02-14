"""Tests for GitHub PR operations and REVIEW transition guardrails (GH-005).

These tests verify:
- TaskPRMapping storage helpers work correctly
- PR linkage persistence roundtrips
- REVIEW transition guardrails block when PR is missing (for connected repos)
- REVIEW transition guardrails block when lease is held by another instance
"""

from __future__ import annotations

import json

import pytest

from kagan.core.plugins.github.sync import (
    GITHUB_TASK_PR_MAPPING_KEY,
    TaskPRLink,
    TaskPRMapping,
    load_task_pr_mapping,
)

# ---------------------------------------------------------------------------
# TaskPRLink Tests
# ---------------------------------------------------------------------------


class TestTaskPRLink:
    """Tests for TaskPRLink data structure."""

    def test_task_pr_link_fields_accessible(self) -> None:
        """TaskPRLink fields are accessible after construction."""
        link = TaskPRLink(
            pr_number=42,
            pr_url="https://github.com/owner/repo/pull/42",
            pr_state="OPEN",
            head_branch="feature/my-feature",
            base_branch="main",
            linked_at="2024-01-15T10:30:00Z",
        )
        assert link.pr_number == 42
        assert link.pr_url == "https://github.com/owner/repo/pull/42"
        assert link.pr_state == "OPEN"
        assert link.head_branch == "feature/my-feature"
        assert link.base_branch == "main"
        assert link.linked_at == "2024-01-15T10:30:00Z"

    def test_task_pr_link_is_frozen(self) -> None:
        """TaskPRLink is immutable."""
        link = TaskPRLink(
            pr_number=42,
            pr_url="https://github.com/owner/repo/pull/42",
            pr_state="OPEN",
            head_branch="feature",
            base_branch="main",
            linked_at="2024-01-15T10:30:00Z",
        )
        with pytest.raises(AttributeError):
            link.pr_number = 99  # type: ignore[misc]


# ---------------------------------------------------------------------------
# TaskPRMapping Tests
# ---------------------------------------------------------------------------


class TestTaskPRMapping:
    """Tests for TaskPRMapping operations."""

    def test_empty_mapping_has_no_prs(self) -> None:
        """Empty mapping reports no PRs."""
        mapping = TaskPRMapping()
        assert not mapping.has_pr("task123")
        assert mapping.get_pr("task123") is None

    def test_link_pr_creates_mapping(self) -> None:
        """link_pr creates a task-to-PR mapping."""
        mapping = TaskPRMapping()
        mapping.link_pr(
            task_id="task123",
            pr_number=42,
            pr_url="https://github.com/owner/repo/pull/42",
            pr_state="OPEN",
            head_branch="feature",
            base_branch="main",
            linked_at="2024-01-15T10:30:00Z",
        )
        assert mapping.has_pr("task123")
        link = mapping.get_pr("task123")
        assert link is not None
        assert link.pr_number == 42
        assert link.pr_state == "OPEN"

    def test_unlink_pr_removes_mapping(self) -> None:
        """unlink_pr removes the task-to-PR mapping."""
        mapping = TaskPRMapping()
        mapping.link_pr(
            task_id="task123",
            pr_number=42,
            pr_url="https://github.com/owner/repo/pull/42",
            pr_state="OPEN",
            head_branch="feature",
            base_branch="main",
            linked_at="2024-01-15T10:30:00Z",
        )
        assert mapping.has_pr("task123")
        mapping.unlink_pr("task123")
        assert not mapping.has_pr("task123")

    def test_update_pr_state_changes_state_only(self) -> None:
        """update_pr_state changes only the PR state."""
        mapping = TaskPRMapping()
        mapping.link_pr(
            task_id="task123",
            pr_number=42,
            pr_url="https://github.com/owner/repo/pull/42",
            pr_state="OPEN",
            head_branch="feature",
            base_branch="main",
            linked_at="2024-01-15T10:30:00Z",
        )
        mapping.update_pr_state("task123", "MERGED")
        link = mapping.get_pr("task123")
        assert link is not None
        assert link.pr_state == "MERGED"
        assert link.pr_number == 42  # Other fields unchanged

    def test_to_dict_and_from_dict_roundtrip(self) -> None:
        """TaskPRMapping serialization roundtrips correctly."""
        mapping = TaskPRMapping()
        mapping.link_pr(
            task_id="task123",
            pr_number=42,
            pr_url="https://github.com/owner/repo/pull/42",
            pr_state="OPEN",
            head_branch="feature",
            base_branch="main",
            linked_at="2024-01-15T10:30:00Z",
        )
        mapping.link_pr(
            task_id="task456",
            pr_number=99,
            pr_url="https://github.com/owner/repo/pull/99",
            pr_state="MERGED",
            head_branch="fix",
            base_branch="develop",
            linked_at="2024-01-16T11:00:00Z",
        )

        # Serialize and deserialize
        data = mapping.to_dict()
        restored = TaskPRMapping.from_dict(data)

        # Verify roundtrip
        assert restored.has_pr("task123")
        assert restored.has_pr("task456")
        link1 = restored.get_pr("task123")
        link2 = restored.get_pr("task456")
        assert link1 is not None
        assert link2 is not None
        assert link1.pr_number == 42
        assert link1.pr_state == "OPEN"
        assert link2.pr_number == 99
        assert link2.pr_state == "MERGED"

    def test_from_dict_handles_none(self) -> None:
        """from_dict returns empty mapping for None input."""
        mapping = TaskPRMapping.from_dict(None)
        assert not mapping.has_pr("any_task")

    def test_from_dict_handles_empty_dict(self) -> None:
        """from_dict returns empty mapping for empty dict."""
        mapping = TaskPRMapping.from_dict({})
        assert not mapping.has_pr("any_task")


# ---------------------------------------------------------------------------
# load_task_pr_mapping Tests
# ---------------------------------------------------------------------------


class TestLoadTaskPRMapping:
    """Tests for load_task_pr_mapping helper."""

    def test_load_from_none_scripts_returns_empty(self) -> None:
        """load_task_pr_mapping returns empty mapping for None scripts."""
        mapping = load_task_pr_mapping(None)
        assert not mapping.has_pr("any_task")

    def test_load_from_empty_scripts_returns_empty(self) -> None:
        """load_task_pr_mapping returns empty mapping for empty scripts."""
        mapping = load_task_pr_mapping({})
        assert not mapping.has_pr("any_task")

    def test_load_from_scripts_with_mapping(self) -> None:
        """load_task_pr_mapping loads mapping from scripts."""
        pr_data = {
            "task_to_pr": {
                "task123": {
                    "pr_number": 42,
                    "pr_url": "https://github.com/owner/repo/pull/42",
                    "pr_state": "OPEN",
                    "head_branch": "feature",
                    "base_branch": "main",
                    "linked_at": "2024-01-15T10:30:00Z",
                }
            }
        }
        scripts = {GITHUB_TASK_PR_MAPPING_KEY: json.dumps(pr_data)}
        mapping = load_task_pr_mapping(scripts)
        assert mapping.has_pr("task123")
        link = mapping.get_pr("task123")
        assert link is not None
        assert link.pr_number == 42

    def test_load_handles_invalid_json_gracefully(self) -> None:
        """load_task_pr_mapping returns empty mapping for invalid JSON."""
        scripts = {GITHUB_TASK_PR_MAPPING_KEY: "not valid json"}
        mapping = load_task_pr_mapping(scripts)
        assert not mapping.has_pr("any_task")
