"""Tests for GitHub issue lease coordination.

These tests focus on user-observable lease behavior:
- active lease contention blocks a second worker without explicit takeover
- stale/orphaned metadata can be reclaimed deterministically
- metadata read failures fail closed and require explicit takeover
"""

from __future__ import annotations

from datetime import datetime, timedelta
from unittest.mock import patch

from kagan.core.plugins.github.lease import (
    LEASE_COMMENT_MARKER,
    LEASE_DURATION_SECONDS,
    LEASE_HELD_BY_OTHER,
    LEASE_LABEL,
    LEASE_STALE_THRESHOLD_SECONDS,
    LeaseHolder,
    acquire_lease,
    build_lease_comment_body,
    create_lease_holder,
    get_lease_state,
    release_lease,
)


def _lease_holder(
    instance_id: str,
    acquired_at: datetime,
    *,
    github_user: str | None = None,
) -> LeaseHolder:
    """Create deterministic lease holder metadata for mocked GitHub comments."""
    hostname, pid_text = instance_id.split(":", 1)
    return LeaseHolder(
        instance_id=instance_id,
        owner_hostname=hostname,
        owner_pid=int(pid_text),
        acquired_at=acquired_at.isoformat(),
        expires_at=(acquired_at + timedelta(seconds=LEASE_DURATION_SECONDS)).isoformat(),
        github_user=github_user,
    )


def _active_holder(instance_id: str, *, github_user: str | None = None) -> LeaseHolder:
    return _lease_holder(instance_id, datetime.now().astimezone(), github_user=github_user)


def _stale_holder(instance_id: str) -> LeaseHolder:
    stale_acquired_at = datetime.now().astimezone() - timedelta(
        seconds=LEASE_DURATION_SECONDS + LEASE_STALE_THRESHOLD_SECONDS + 10
    )
    return _lease_holder(instance_id, stale_acquired_at)


def _lease_comment(holder: LeaseHolder, *, comment_id: int) -> dict[str, object]:
    return {"id": comment_id, "body": build_lease_comment_body(holder)}


class TestLeaseCoordination:
    """Behavior tests around contention, takeover, renew, and reclaim flows."""

    def test_second_instance_blocked_without_takeover(self) -> None:
        holder = _active_holder("host1:1000", github_user="user1")

        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_view",
                return_value=({"labels": [{"name": LEASE_LABEL}]}, None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_issue_comments",
                return_value=([_lease_comment(holder, comment_id=17)], None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_comment_create"
            ) as create_comment,
        ):
            result = acquire_lease(
                "/usr/bin/gh",
                "/tmp/repo",
                "owner",
                "repo",
                42,
                force_takeover=False,
            )

        assert result.success is False
        assert result.code == LEASE_HELD_BY_OTHER
        assert result.holder is not None
        assert result.holder.instance_id == "host1:1000"
        assert result.holder.github_user == "user1"
        assert "host1:1000" in result.message
        create_comment.assert_not_called()

    def test_force_takeover_replaces_active_lease_metadata(self) -> None:
        active_holder = _active_holder("otherhost:777")

        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_view",
                return_value=({"labels": [{"name": LEASE_LABEL}]}, None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_issue_comments",
                return_value=([_lease_comment(active_holder, comment_id=88)], None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_comment_delete"
            ) as delete_comment,
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_label_add",
                return_value=(True, None),
            ) as add_label,
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_comment_create",
                return_value=(501, None),
            ) as create_comment,
        ):
            result = acquire_lease(
                "/usr/bin/gh",
                "/tmp/repo",
                "owner",
                "repo",
                42,
                github_user="responder",
                force_takeover=True,
            )

        assert result.success is True
        assert result.code == "LEASE_ACQUIRED"
        delete_comment.assert_called_once_with("/usr/bin/gh", "/tmp/repo", "owner", "repo", 88)
        add_label.assert_not_called()
        create_comment.assert_called_once()
        created_comment_body = create_comment.call_args.args[3]
        assert LEASE_COMMENT_MARKER in created_comment_body
        assert '"github_user": "responder"' in created_comment_body

    def test_stale_lease_reclaim_succeeds_without_force_takeover(self) -> None:
        stale_holder = _stale_holder("deadhost:9999")

        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_view",
                return_value=({"labels": [{"name": LEASE_LABEL}]}, None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_issue_comments",
                return_value=([_lease_comment(stale_holder, comment_id=31)], None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_comment_delete"
            ) as delete_comment,
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_comment_create",
                return_value=(777, None),
            ) as create_comment,
        ):
            result = acquire_lease(
                "/usr/bin/gh",
                "/tmp/repo",
                "owner",
                "repo",
                42,
                force_takeover=False,
            )

        assert result.success is True
        assert result.code == "LEASE_ACQUIRED"
        delete_comment.assert_called_once_with("/usr/bin/gh", "/tmp/repo", "owner", "repo", 31)
        create_comment.assert_called_once()

    def test_current_holder_renews_instead_of_competing(self) -> None:
        same_instance_holder = create_lease_holder(github_user="old-user")

        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_view",
                return_value=({"labels": [{"name": LEASE_LABEL}]}, None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_issue_comments",
                return_value=([_lease_comment(same_instance_holder, comment_id=92)], None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_comment_delete"
            ) as delete_comment,
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_comment_create",
                return_value=(802, None),
            ) as create_comment,
        ):
            result = acquire_lease(
                "/usr/bin/gh",
                "/tmp/repo",
                "owner",
                "repo",
                42,
                github_user="new-user",
            )

        assert result.success is True
        assert result.code == "LEASE_RENEWED"
        assert result.holder is not None
        assert result.holder.github_user == "new-user"
        delete_comment.assert_called_once_with("/usr/bin/gh", "/tmp/repo", "owner", "repo", 92)
        create_comment.assert_called_once()

    def test_current_holder_does_not_delete_old_comment_when_renewal_comment_fails(self) -> None:
        same_instance_holder = create_lease_holder(github_user="old-user")

        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_view",
                return_value=({"labels": [{"name": LEASE_LABEL}]}, None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_issue_comments",
                return_value=([_lease_comment(same_instance_holder, comment_id=92)], None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_comment_delete"
            ) as delete_comment,
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_comment_create",
                return_value=(None, "comment create failed"),
            ) as create_comment,
        ):
            result = acquire_lease(
                "/usr/bin/gh",
                "/tmp/repo",
                "owner",
                "repo",
                42,
                github_user="new-user",
            )

        assert result.success is False
        assert result.code == "LEASE_ACQUIRE_FAILED"
        assert "Failed to create lease comment" in result.message
        create_comment.assert_called_once()
        delete_comment.assert_not_called()

    def test_acquire_free_issue_adds_label_and_comment(self) -> None:
        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_view",
                return_value=({"labels": []}, None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_issue_comments"
            ) as list_comments,
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_label_add",
                return_value=(True, None),
            ) as add_label,
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_comment_create",
                return_value=(120, None),
            ) as create_comment,
        ):
            result = acquire_lease(
                "/usr/bin/gh",
                "/tmp/repo",
                "owner",
                "repo",
                42,
                github_user="owner1",
            )

        assert result.success is True
        assert result.code == "LEASE_ACQUIRED"
        list_comments.assert_not_called()
        add_label.assert_called_once_with("/usr/bin/gh", "/tmp/repo", 42, LEASE_LABEL)
        create_comment.assert_called_once()
        created_comment_body = create_comment.call_args.args[3]
        assert LEASE_COMMENT_MARKER in created_comment_body

    def test_acquire_free_issue_rolls_back_label_when_comment_create_fails(self) -> None:
        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_view",
                return_value=({"labels": []}, None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_issue_comments"
            ) as list_comments,
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_label_add",
                return_value=(True, None),
            ) as add_label,
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_label_remove",
                return_value=(True, None),
            ) as remove_label,
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_comment_create",
                return_value=(None, "comment create failed"),
            ) as create_comment,
        ):
            result = acquire_lease(
                "/usr/bin/gh",
                "/tmp/repo",
                "owner",
                "repo",
                42,
                github_user="owner1",
            )

        assert result.success is False
        assert result.code == "LEASE_ACQUIRE_FAILED"
        list_comments.assert_not_called()
        add_label.assert_called_once_with("/usr/bin/gh", "/tmp/repo", 42, LEASE_LABEL)
        create_comment.assert_called_once()
        remove_label.assert_called_once_with("/usr/bin/gh", "/tmp/repo", 42, LEASE_LABEL)


class TestLeaseMetadataRecovery:
    """Regression tests for metadata read failures and malformed metadata."""

    def test_get_lease_state_marks_metadata_incomplete_when_comments_unavailable(self) -> None:
        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_view",
                return_value=({"labels": [{"name": LEASE_LABEL}]}, None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_issue_comments",
                return_value=(None, "api unavailable"),
            ),
        ):
            state, error = get_lease_state(
                "/usr/bin/gh",
                "/tmp/repo",
                "owner",
                "repo",
                42,
            )

        assert error is None
        assert state is not None
        assert state.has_label
        assert state.metadata_complete is False
        assert state.requires_takeover
        assert not state.can_acquire

    def test_acquire_lease_requires_explicit_takeover_when_metadata_is_incomplete(self) -> None:
        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_view",
                return_value=({"labels": [{"name": LEASE_LABEL}]}, None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_issue_comments",
                return_value=(None, "api unavailable"),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_comment_create"
            ) as create_comment,
        ):
            result = acquire_lease(
                "/usr/bin/gh",
                "/tmp/repo",
                "owner",
                "repo",
                42,
                force_takeover=False,
            )

        assert result.success is False
        assert result.code == LEASE_HELD_BY_OTHER
        assert "force_takeover=true" in result.message
        create_comment.assert_not_called()

    def test_acquire_lease_force_takeover_proceeds_when_metadata_is_incomplete(self) -> None:
        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_view",
                return_value=({"labels": [{"name": LEASE_LABEL}]}, None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_issue_comments",
                return_value=(None, "api unavailable"),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_comment_create",
                return_value=(404, None),
            ) as create_comment,
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_label_add",
                return_value=(True, None),
            ) as add_label,
        ):
            result = acquire_lease(
                "/usr/bin/gh",
                "/tmp/repo",
                "owner",
                "repo",
                42,
                force_takeover=True,
            )

        assert result.success is True
        assert result.code == "LEASE_ACQUIRED"
        add_label.assert_not_called()
        create_comment.assert_called_once()

    def test_invalid_lease_comment_falls_back_to_orphan_lock_behavior(self) -> None:
        invalid_comment = f"{LEASE_COMMENT_MARKER}\n```json\n{{invalid json}}\n```"

        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_issue_view",
                return_value=({"labels": [{"name": LEASE_LABEL}]}, None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_api_issue_comments",
                return_value=([{"id": 1, "body": invalid_comment}], None),
            ),
        ):
            state, error = get_lease_state(
                "/usr/bin/gh",
                "/tmp/repo",
                "owner",
                "repo",
                42,
            )

        assert error is None
        assert state is not None
        assert state.has_label
        assert state.metadata_complete is True
        assert state.holder is None
        assert state.comment_id is None
        assert state.can_acquire
        assert not state.requires_takeover


def test_release_lease_returns_safe_error_when_not_lease_holder() -> None:
    active_holder = _active_holder("host1:1000")

    with (
        patch(
            "kagan.core.plugins.github.gh_adapter.run_gh_issue_view",
            return_value=({"labels": [{"name": LEASE_LABEL}]}, None),
        ),
        patch(
            "kagan.core.plugins.github.gh_adapter.run_gh_api_issue_comments",
            return_value=([_lease_comment(active_holder, comment_id=7)], None),
        ),
        patch("kagan.core.plugins.github.gh_adapter.run_gh_issue_label_remove") as remove_label,
        patch("kagan.core.plugins.github.gh_adapter.run_gh_api_comment_delete") as delete_comment,
    ):
        result = release_lease(
            "/usr/bin/gh",
            "/tmp/repo",
            "owner",
            "repo",
            42,
        )

    assert result.success is False
    assert result.code == "LEASE_NOT_HELD"
    assert "not held by this instance" in result.message
    remove_label.assert_not_called()
    delete_comment.assert_not_called()
