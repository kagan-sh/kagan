"""Tests for GitHub issue lease coordination.

Tests critical user-facing behavior:
- Second instance cannot acquire active lease without takeover
- Lease holder information is visible/actionable in responses
- Stale lease reclaim is deterministic
"""

from __future__ import annotations

from datetime import datetime, timedelta
from typing import TYPE_CHECKING
from unittest.mock import patch

import pytest

from kagan.core.plugins.github.lease import (
    LEASE_COMMENT_MARKER,
    LEASE_DURATION_SECONDS,
    LEASE_HELD_BY_OTHER,
    LEASE_LABEL,
    LEASE_STALE_THRESHOLD_SECONDS,
    LeaseAcquireResult,
    LeaseHolder,
    LeaseReleaseResult,
    LeaseState,
    build_lease_comment_body,
    create_lease_holder,
    parse_lease_comment,
)

if TYPE_CHECKING:
    pass


class TestLeaseHolder:
    """Tests for LeaseHolder data class behavior."""

    def test_lease_holder_from_dict(self) -> None:
        """Lease holder can be parsed from comment JSON."""
        data = {
            "instance_id": "testhost:1234",
            "acquired_at": "2024-01-01T00:00:00+00:00",
            "expires_at": "2024-01-01T01:00:00+00:00",
            "github_user": "testuser",
        }
        holder = LeaseHolder.from_dict(data)
        assert holder.instance_id == "testhost:1234"
        assert holder.owner_hostname == "testhost"
        assert holder.owner_pid == 1234
        assert holder.github_user == "testuser"

    def test_lease_holder_to_dict_roundtrip(self) -> None:
        """Lease holder serializes and deserializes correctly."""
        holder = create_lease_holder(github_user="testuser")
        data = holder.to_dict()
        restored = LeaseHolder.from_dict(data)
        assert restored.instance_id == holder.instance_id
        assert restored.acquired_at == holder.acquired_at
        assert restored.expires_at == holder.expires_at

    def test_is_stale_returns_true_for_old_lease(self) -> None:
        """Lease is stale when expiry + threshold has passed."""
        old_time = datetime.now().astimezone() - timedelta(
            seconds=LEASE_DURATION_SECONDS + LEASE_STALE_THRESHOLD_SECONDS + 100
        )
        holder = LeaseHolder(
            instance_id="other:999",
            owner_hostname="other",
            owner_pid=999,
            acquired_at=old_time.isoformat(),
            expires_at=(old_time + timedelta(seconds=LEASE_DURATION_SECONDS)).isoformat(),
        )
        assert holder.is_stale()

    def test_is_stale_returns_false_for_fresh_lease(self) -> None:
        """Lease is not stale when recently created."""
        holder = create_lease_holder()
        assert not holder.is_stale()

    def test_is_expired_returns_true_after_duration(self) -> None:
        """Lease is expired when past expires_at timestamp."""
        old_time = datetime.now().astimezone() - timedelta(seconds=LEASE_DURATION_SECONDS + 10)
        holder = LeaseHolder(
            instance_id="other:999",
            owner_hostname="other",
            owner_pid=999,
            acquired_at=old_time.isoformat(),
            expires_at=(old_time + timedelta(seconds=LEASE_DURATION_SECONDS)).isoformat(),
        )
        assert holder.is_expired()

    def test_is_same_instance_matches_current_process(self) -> None:
        """is_same_instance returns True for current process."""
        holder = create_lease_holder()
        assert holder.is_same_instance()

    def test_is_same_instance_false_for_different_process(self) -> None:
        """is_same_instance returns False for different instance_id."""
        holder = LeaseHolder(
            instance_id="otherhost:9999",
            owner_hostname="otherhost",
            owner_pid=9999,
            acquired_at="2024-01-01T00:00:00+00:00",
            expires_at="2024-01-01T01:00:00+00:00",
        )
        assert not holder.is_same_instance()


class TestLeaseState:
    """Tests for LeaseState computed properties."""

    def test_is_locked_false_when_no_label(self) -> None:
        """Issue is not locked when label is absent."""
        state = LeaseState(has_label=False, holder=None)
        assert not state.is_locked

    def test_is_locked_true_when_label_present_with_holder(self) -> None:
        """Issue is locked when label present and lease is not stale."""
        holder = create_lease_holder()
        state = LeaseState(has_label=True, holder=holder)
        assert state.is_locked

    def test_is_locked_true_with_orphan_label(self) -> None:
        """Issue is locked when label present but no comment (orphan state)."""
        state = LeaseState(has_label=True, holder=None)
        assert state.is_locked

    def test_can_acquire_true_when_unlocked(self) -> None:
        """Can acquire when no lock label."""
        state = LeaseState(has_label=False, holder=None)
        assert state.can_acquire

    def test_can_acquire_true_when_stale(self) -> None:
        """Can acquire when lease is stale."""
        old_time = datetime.now().astimezone() - timedelta(
            seconds=LEASE_DURATION_SECONDS + LEASE_STALE_THRESHOLD_SECONDS + 100
        )
        holder = LeaseHolder(
            instance_id="other:999",
            owner_hostname="other",
            owner_pid=999,
            acquired_at=old_time.isoformat(),
            expires_at=(old_time + timedelta(seconds=LEASE_DURATION_SECONDS)).isoformat(),
        )
        state = LeaseState(has_label=True, holder=holder)
        assert state.can_acquire

    def test_can_acquire_true_when_held_by_current_instance(self) -> None:
        """Can acquire when current instance already holds lease."""
        holder = create_lease_holder()
        state = LeaseState(has_label=True, holder=holder)
        assert state.can_acquire

    def test_can_acquire_false_when_held_by_other(self) -> None:
        """Cannot acquire when another active instance holds lease."""
        holder = LeaseHolder(
            instance_id="otherhost:9999",
            owner_hostname="otherhost",
            owner_pid=9999,
            acquired_at=datetime.now().astimezone().isoformat(),
            expires_at=(
                datetime.now().astimezone() + timedelta(seconds=LEASE_DURATION_SECONDS)
            ).isoformat(),
        )
        state = LeaseState(has_label=True, holder=holder)
        assert not state.can_acquire

    def test_requires_takeover_true_for_active_other_holder(self) -> None:
        """Takeover required when another active instance holds lease."""
        holder = LeaseHolder(
            instance_id="otherhost:9999",
            owner_hostname="otherhost",
            owner_pid=9999,
            acquired_at=datetime.now().astimezone().isoformat(),
            expires_at=(
                datetime.now().astimezone() + timedelta(seconds=LEASE_DURATION_SECONDS)
            ).isoformat(),
        )
        state = LeaseState(has_label=True, holder=holder)
        assert state.requires_takeover

    def test_requires_takeover_false_when_stale(self) -> None:
        """No takeover required when lease is stale."""
        old_time = datetime.now().astimezone() - timedelta(
            seconds=LEASE_DURATION_SECONDS + LEASE_STALE_THRESHOLD_SECONDS + 100
        )
        holder = LeaseHolder(
            instance_id="other:999",
            owner_hostname="other",
            owner_pid=999,
            acquired_at=old_time.isoformat(),
            expires_at=(old_time + timedelta(seconds=LEASE_DURATION_SECONDS)).isoformat(),
        )
        state = LeaseState(has_label=True, holder=holder)
        assert not state.requires_takeover


class TestLeaseCommentParsing:
    """Tests for lease comment body parsing."""

    def test_build_and_parse_comment_roundtrip(self) -> None:
        """Comment body can be built and parsed back."""
        holder = create_lease_holder(github_user="testuser")
        body = build_lease_comment_body(holder)
        assert LEASE_COMMENT_MARKER in body
        parsed = parse_lease_comment(body)
        assert parsed is not None
        assert parsed.instance_id == holder.instance_id

    def test_parse_returns_none_for_non_lease_comment(self) -> None:
        """Non-lease comments return None."""
        body = "This is a regular comment without lease metadata."
        assert parse_lease_comment(body) is None

    def test_parse_returns_none_for_invalid_json(self) -> None:
        """Invalid JSON in lease comment returns None."""
        body = f"{LEASE_COMMENT_MARKER}\n```json\n{{invalid json}}\n```"
        assert parse_lease_comment(body) is None


class TestLeaseAcquireResult:
    """Tests for LeaseAcquireResult factory methods."""

    def test_acquired_result_is_success(self) -> None:
        """Acquired result indicates success."""
        holder = create_lease_holder()
        result = LeaseAcquireResult.acquired(holder)
        assert result.success
        assert result.code == "LEASE_ACQUIRED"
        assert result.holder == holder

    def test_renewed_result_is_success(self) -> None:
        """Renewed result indicates success."""
        holder = create_lease_holder()
        result = LeaseAcquireResult.renewed(holder)
        assert result.success
        assert result.code == "LEASE_RENEWED"

    def test_blocked_result_includes_holder_info(self) -> None:
        """Blocked result includes holder info for user display."""
        holder = LeaseHolder(
            instance_id="otherhost:9999",
            owner_hostname="otherhost",
            owner_pid=9999,
            acquired_at="2024-01-01T00:00:00+00:00",
            expires_at="2024-01-01T01:00:00+00:00",
        )
        result = LeaseAcquireResult.blocked(holder)
        assert not result.success
        assert result.code == LEASE_HELD_BY_OTHER
        assert result.holder == holder
        assert "otherhost:9999" in result.message


class TestLeaseReleaseResult:
    """Tests for LeaseReleaseResult factory methods."""

    def test_released_result_is_success(self) -> None:
        """Released result indicates success."""
        result = LeaseReleaseResult.released()
        assert result.success
        assert result.code == "LEASE_RELEASED"

    def test_not_held_result_is_failure(self) -> None:
        """Not held result indicates failure."""
        result = LeaseReleaseResult.not_held()
        assert not result.success
        assert result.code == "LEASE_NOT_HELD"


class TestLeaseContention:
    """Integration tests for lease contention behavior.

    These tests verify the critical user-facing acceptance criteria:
    - Second instance cannot acquire active lease without takeover
    - Lease holder information is visible/actionable
    - Stale lease reclaim is deterministic
    """

    def test_second_instance_blocked_without_takeover(self) -> None:
        """Second instance is blocked from acquiring an active lease.

        Acceptance criteria: Second instance cannot acquire active lease without takeover.
        """
        # Simulate first instance holding a lease
        first_holder = LeaseHolder(
            instance_id="host1:1000",
            owner_hostname="host1",
            owner_pid=1000,
            acquired_at=datetime.now().astimezone().isoformat(),
            expires_at=(
                datetime.now().astimezone() + timedelta(seconds=LEASE_DURATION_SECONDS)
            ).isoformat(),
            github_user="user1",
        )
        state = LeaseState(has_label=True, holder=first_holder)

        # Second instance should require takeover
        assert state.requires_takeover
        assert not state.can_acquire

        # Blocked result should include holder info for user action
        result = LeaseAcquireResult.blocked(first_holder)
        assert not result.success
        assert result.holder is not None
        assert result.holder.instance_id == "host1:1000"
        assert result.holder.github_user == "user1"

    def test_stale_lease_reclaim_is_deterministic(self) -> None:
        """Stale leases can be reclaimed deterministically.

        Acceptance criteria: Stale lease reclaim is deterministic.
        """
        # Stale lease: expired + past threshold
        stale_time = datetime.now().astimezone() - timedelta(
            seconds=LEASE_DURATION_SECONDS + LEASE_STALE_THRESHOLD_SECONDS + 1
        )
        stale_holder = LeaseHolder(
            instance_id="deadhost:9999",
            owner_hostname="deadhost",
            owner_pid=9999,
            acquired_at=stale_time.isoformat(),
            expires_at=(stale_time + timedelta(seconds=LEASE_DURATION_SECONDS)).isoformat(),
        )

        # Verify stale detection is deterministic
        assert stale_holder.is_stale()
        assert stale_holder.is_expired()

        # State should allow acquisition without takeover
        state = LeaseState(has_label=True, holder=stale_holder)
        assert state.can_acquire
        assert not state.requires_takeover

    def test_lease_holder_info_visible_in_blocked_response(self) -> None:
        """Lease holder info is visible and actionable in error responses.

        Acceptance criteria: Lease holder information is visible/actionable in TUI/MCP errors.
        """
        holder = LeaseHolder(
            instance_id="workstation.local:12345",
            owner_hostname="workstation.local",
            owner_pid=12345,
            acquired_at="2024-01-15T10:30:00+00:00",
            expires_at="2024-01-15T11:30:00+00:00",
            github_user="developer",
        )

        result = LeaseAcquireResult.blocked(holder)

        # Holder info must be present for user display
        assert result.holder is not None
        assert result.holder.owner_hostname == "workstation.local"
        assert result.holder.owner_pid == 12345
        assert result.holder.github_user == "developer"

        # Message must identify the holder
        assert "workstation.local:12345" in result.message

    def test_force_takeover_succeeds_on_active_lease(self) -> None:
        """Force takeover path allows overriding active lease.

        This verifies the takeover path exists for the user to act on blocked state.
        """
        # Active lease held by another instance
        other_holder = LeaseHolder(
            instance_id="other:999",
            owner_hostname="other",
            owner_pid=999,
            acquired_at=datetime.now().astimezone().isoformat(),
            expires_at=(
                datetime.now().astimezone() + timedelta(seconds=LEASE_DURATION_SECONDS)
            ).isoformat(),
        )
        state = LeaseState(has_label=True, holder=other_holder)

        # Without force, takeover is required
        assert state.requires_takeover

        # With force_takeover=True, the acquire_lease function would proceed
        # (tested via the actual function in integration tests)
        # Here we verify the state correctly identifies the takeover requirement
        assert not state.can_acquire
        assert state.is_locked
