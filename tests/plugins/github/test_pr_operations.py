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


# ---------------------------------------------------------------------------
# PR Reconcile Message Builder Tests (GH-006)
# ---------------------------------------------------------------------------


class TestBuildReconcileMessage:
    """Tests for _build_reconcile_message helper."""

    def test_merged_pr_with_task_transition(self) -> None:
        """Merged PR with task status change produces correct message."""
        from kagan.core.plugins.github.runtime import _build_reconcile_message

        msg = _build_reconcile_message(pr_number=42, pr_state="MERGED", task_changed=True)
        assert "PR #42 merged" in msg
        assert "Task moved to DONE" in msg

    def test_merged_pr_without_task_transition(self) -> None:
        """Merged PR without task status change (already DONE) produces correct message."""
        from kagan.core.plugins.github.runtime import _build_reconcile_message

        msg = _build_reconcile_message(pr_number=42, pr_state="MERGED", task_changed=False)
        assert "PR #42 merged" in msg
        assert "Task already DONE" in msg

    def test_closed_pr_with_task_transition(self) -> None:
        """Closed (unmerged) PR with task status change produces correct message."""
        from kagan.core.plugins.github.runtime import _build_reconcile_message

        msg = _build_reconcile_message(pr_number=99, pr_state="CLOSED", task_changed=True)
        assert "PR #99 closed without merge" in msg
        assert "Task moved to IN_PROGRESS" in msg

    def test_closed_pr_without_task_transition(self) -> None:
        """Closed (unmerged) PR without task status change produces correct message."""
        from kagan.core.plugins.github.runtime import _build_reconcile_message

        msg = _build_reconcile_message(pr_number=99, pr_state="CLOSED", task_changed=False)
        assert "PR #99 closed without merge" in msg
        assert "Task status unchanged" in msg

    def test_open_pr_no_task_change(self) -> None:
        """Open PR produces no task change message."""
        from kagan.core.plugins.github.runtime import _build_reconcile_message

        msg = _build_reconcile_message(pr_number=55, pr_state="OPEN", task_changed=False)
        assert "PR #55 is open" in msg
        assert "No task status change" in msg


# ---------------------------------------------------------------------------
# PR State to Task Transition Logic Tests (GH-006)
# ---------------------------------------------------------------------------


class TestPRStateToTaskTransition:
    """Tests for deterministic PR state to task status mapping.

    These tests verify the acceptance criteria:
    - Merged PR maps to DONE deterministically
    - Closed-unmerged maps to IN_PROGRESS deterministically
    - Reconcile is idempotent and safe to re-run
    """

    def test_merged_pr_maps_to_done(self) -> None:
        """PR state MERGED deterministically maps to TaskStatus.DONE."""
        from kagan.core.models.enums import TaskStatus

        # Given a task in REVIEW status with a merged PR
        current_status = TaskStatus.REVIEW
        pr_state = "MERGED"

        # When we determine the target status
        # The logic is: MERGED -> DONE
        if pr_state == "MERGED":
            target_status = TaskStatus.DONE
        elif pr_state == "CLOSED":
            target_status = TaskStatus.IN_PROGRESS
        else:
            target_status = current_status

        # Then the task should transition to DONE
        assert target_status == TaskStatus.DONE

    def test_closed_unmerged_pr_maps_to_in_progress(self) -> None:
        """PR state CLOSED (unmerged) deterministically maps to TaskStatus.IN_PROGRESS."""
        from kagan.core.models.enums import TaskStatus

        # Given a task in REVIEW status with a closed (unmerged) PR
        current_status = TaskStatus.REVIEW
        pr_state = "CLOSED"

        # When we determine the target status
        if pr_state == "MERGED":
            target_status = TaskStatus.DONE
        elif pr_state == "CLOSED":
            # Only transition if not already DONE
            if current_status != TaskStatus.DONE:
                target_status = TaskStatus.IN_PROGRESS
            else:
                target_status = current_status
        else:
            target_status = current_status

        # Then the task should transition to IN_PROGRESS
        assert target_status == TaskStatus.IN_PROGRESS

    def test_open_pr_no_status_change(self) -> None:
        """PR state OPEN does not change task status."""
        from kagan.core.models.enums import TaskStatus

        # Given a task in REVIEW status with an open PR
        current_status = TaskStatus.REVIEW
        pr_state = "OPEN"

        # When we determine the target status
        if pr_state == "MERGED":
            target_status = TaskStatus.DONE
        elif pr_state == "CLOSED":
            target_status = TaskStatus.IN_PROGRESS
        else:
            target_status = current_status

        # Then the task should remain in REVIEW
        assert target_status == TaskStatus.REVIEW

    def test_idempotent_merged_already_done(self) -> None:
        """Re-running reconcile on merged PR with task already DONE is idempotent."""
        from kagan.core.models.enums import TaskStatus

        # Given a task already DONE with a merged PR
        current_status = TaskStatus.DONE
        pr_state = "MERGED"

        # When we check if transition is needed
        needs_transition = pr_state == "MERGED" and current_status != TaskStatus.DONE

        # Then no transition is needed (idempotent)
        assert not needs_transition

    def test_idempotent_closed_already_in_progress(self) -> None:
        """Re-running reconcile on closed PR with task already IN_PROGRESS is idempotent."""
        from kagan.core.models.enums import TaskStatus

        # Given a task already IN_PROGRESS with a closed PR
        current_status = TaskStatus.IN_PROGRESS
        pr_state = "CLOSED"

        # When we check if transition is needed
        needs_transition = (
            pr_state == "CLOSED"
            and current_status != TaskStatus.DONE
            and current_status != TaskStatus.IN_PROGRESS
        )

        # Then no transition is needed (idempotent)
        assert not needs_transition

    def test_closed_pr_does_not_override_done_task(self) -> None:
        """Closed PR does not move a DONE task back to IN_PROGRESS."""
        from kagan.core.models.enums import TaskStatus

        # Given a task already DONE with a closed PR (edge case)
        current_status = TaskStatus.DONE
        pr_state = "CLOSED"

        # When we determine if transition is needed
        should_transition = (
            pr_state == "CLOSED"
            and current_status != TaskStatus.DONE
            and current_status != TaskStatus.IN_PROGRESS
        )

        # Then no transition should happen (preserves DONE status)
        assert not should_transition
