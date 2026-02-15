"""GitHub issue lease coordination via labels and comments.

Only one active Kagan instance can work a GitHub issue at a time by default.
Uses label `kagan:locked` as lock signal and a marker comment for lock holder metadata.
"""

from __future__ import annotations

import json
import os
import re
import socket
from dataclasses import dataclass
from datetime import datetime, timedelta
from typing import Any, Final

from kagan.core.time import utc_now

# Lease label applied to issues under active work
LEASE_LABEL: Final = "kagan:locked"

# Default lease duration before considered stale
LEASE_DURATION_SECONDS: Final = 3600  # 1 hour
LEASE_STALE_THRESHOLD_SECONDS: Final = 7200  # 2 hours (stale if no renewal)

# Comment marker to identify lease metadata comments
LEASE_COMMENT_MARKER: Final = "<!-- kagan:lease:v1 -->"
LEASE_COMMENT_PATTERN: Final = re.compile(
    rf"{re.escape(LEASE_COMMENT_MARKER)}\s*```json\s*(\{{.*?\}})\s*```",
    re.DOTALL,
)

# Error codes for lease operations
LEASE_HELD_BY_OTHER: Final = "LEASE_HELD_BY_OTHER"
LEASE_ACQUIRE_FAILED: Final = "LEASE_ACQUIRE_FAILED"
LEASE_RELEASE_FAILED: Final = "LEASE_RELEASE_FAILED"
LEASE_NOT_HELD: Final = "LEASE_NOT_HELD"


def _generate_instance_id() -> str:
    """Generate a unique instance identifier: hostname:pid."""
    return f"{socket.gethostname()}:{os.getpid()}"


@dataclass(frozen=True, slots=True)
class LeaseHolder:
    """Information about the current lease holder."""

    instance_id: str
    owner_hostname: str
    owner_pid: int
    acquired_at: str
    expires_at: str
    github_user: str | None = None

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> LeaseHolder:
        """Parse LeaseHolder from lease comment JSON."""
        instance_id = str(data.get("instance_id", ""))
        parts = instance_id.split(":", 1)
        hostname = parts[0] if parts else "unknown"
        pid = 0
        if len(parts) > 1:
            try:
                pid = int(parts[1])
            except (TypeError, ValueError):
                pid = 0
        return cls(
            instance_id=instance_id,
            owner_hostname=hostname,
            owner_pid=pid,
            acquired_at=str(data.get("acquired_at", "")),
            expires_at=str(data.get("expires_at", "")),
            github_user=(
                str(data["github_user"])
                if isinstance(data.get("github_user"), str) and data.get("github_user")
                else None
            ),
        )

    def to_dict(self) -> dict[str, Any]:
        """Serialize LeaseHolder to dict for JSON storage."""
        result: dict[str, Any] = {
            "instance_id": self.instance_id,
            "acquired_at": self.acquired_at,
            "expires_at": self.expires_at,
        }
        if self.github_user:
            result["github_user"] = self.github_user
        return result

    def is_stale(self, now: datetime | None = None) -> bool:
        """Check if the lease is stale (expired and past grace period)."""
        if now is None:
            now = utc_now()
        try:
            expires = datetime.fromisoformat(self.expires_at)
            # Add stale threshold to expiry for grace period
            stale_at = expires + timedelta(seconds=LEASE_STALE_THRESHOLD_SECONDS)
            return now > stale_at
        except (ValueError, TypeError):
            # Invalid timestamp means stale
            return True

    def is_expired(self, now: datetime | None = None) -> bool:
        """Check if the lease is expired (but may not be stale yet)."""
        if now is None:
            now = utc_now()
        try:
            expires = datetime.fromisoformat(self.expires_at)
            return now > expires
        except (ValueError, TypeError):
            return True

    def is_same_instance(self) -> bool:
        """Check if this lease is held by the current instance."""
        return self.instance_id == _generate_instance_id()


@dataclass(frozen=True, slots=True)
class LeaseState:
    """Current state of a GitHub issue lease."""

    has_label: bool
    holder: LeaseHolder | None
    comment_id: int | None = None
    metadata_complete: bool = True

    @property
    def is_locked(self) -> bool:
        """True if the issue has an active (non-stale) lease."""
        if not self.has_label:
            return False
        if self.holder is None:
            # Label present but no comment - treat as locked (orphan state)
            return True
        return not self.holder.is_stale()

    @property
    def is_held_by_current_instance(self) -> bool:
        """True if current instance holds the lease."""
        if not self.is_locked:
            return False
        if self.holder is None:
            return False
        return self.holder.is_same_instance()

    @property
    def can_acquire(self) -> bool:
        """True if the current instance can acquire (or already holds) the lease."""
        if not self.has_label:
            return True
        if not self.metadata_complete:
            return False
        if self.holder is None:
            # Orphan label - allow takeover
            return True
        if self.holder.is_same_instance():
            return True
        return self.holder.is_stale()

    @property
    def requires_takeover(self) -> bool:
        """True if acquisition requires explicit takeover."""
        if not self.has_label:
            return False
        if not self.metadata_complete:
            return True
        if self.holder is None:
            return False
        if self.holder.is_same_instance():
            return False
        # Active lease held by another instance
        return not self.holder.is_stale()


@dataclass(frozen=True, slots=True)
class LeaseAcquireResult:
    """Result of attempting to acquire a lease."""

    success: bool
    code: str
    message: str
    holder: LeaseHolder | None = None
    comment_id: int | None = None

    @classmethod
    def acquired(cls, holder: LeaseHolder, comment_id: int | None = None) -> LeaseAcquireResult:
        return cls(
            success=True,
            code="LEASE_ACQUIRED",
            message="Lease acquired successfully",
            holder=holder,
            comment_id=comment_id,
        )

    @classmethod
    def renewed(cls, holder: LeaseHolder, comment_id: int | None = None) -> LeaseAcquireResult:
        return cls(
            success=True,
            code="LEASE_RENEWED",
            message="Lease renewed successfully",
            holder=holder,
            comment_id=comment_id,
        )

    @classmethod
    def blocked(cls, holder: LeaseHolder) -> LeaseAcquireResult:
        return cls(
            success=False,
            code=LEASE_HELD_BY_OTHER,
            message=f"Lease held by {holder.instance_id} (acquired {holder.acquired_at})",
            holder=holder,
        )

    @classmethod
    def takeover_required(cls, message: str) -> LeaseAcquireResult:
        return cls(
            success=False,
            code=LEASE_HELD_BY_OTHER,
            message=message,
        )

    @classmethod
    def failed(cls, message: str) -> LeaseAcquireResult:
        return cls(
            success=False,
            code=LEASE_ACQUIRE_FAILED,
            message=message,
        )


@dataclass(frozen=True, slots=True)
class LeaseReleaseResult:
    """Result of attempting to release a lease."""

    success: bool
    code: str
    message: str

    @classmethod
    def released(cls) -> LeaseReleaseResult:
        return cls(success=True, code="LEASE_RELEASED", message="Lease released successfully")

    @classmethod
    def not_held(cls) -> LeaseReleaseResult:
        return cls(
            success=False,
            code=LEASE_NOT_HELD,
            message="Cannot release lease not held by this instance",
        )

    @classmethod
    def failed(cls, message: str) -> LeaseReleaseResult:
        return cls(success=False, code=LEASE_RELEASE_FAILED, message=message)


def build_lease_comment_body(holder: LeaseHolder) -> str:
    """Build the lease marker comment body with embedded JSON metadata."""
    return f"""{LEASE_COMMENT_MARKER}
```json
{json.dumps(holder.to_dict(), indent=2)}
```

This issue is currently being worked on by a Kagan instance.
"""


def parse_lease_comment(body: str) -> LeaseHolder | None:
    """Parse lease holder metadata from a comment body."""
    match = LEASE_COMMENT_PATTERN.search(body)
    if not match:
        return None
    try:
        data = json.loads(match.group(1))
        return LeaseHolder.from_dict(data)
    except (json.JSONDecodeError, TypeError, ValueError, KeyError):
        return None


def create_lease_holder(
    github_user: str | None = None,
    duration_seconds: int = LEASE_DURATION_SECONDS,
) -> LeaseHolder:
    """Create a new LeaseHolder for the current instance."""
    now = utc_now()
    expires = now + timedelta(seconds=duration_seconds)
    instance_id = _generate_instance_id()
    parts = instance_id.split(":", 1)
    return LeaseHolder(
        instance_id=instance_id,
        owner_hostname=parts[0],
        owner_pid=int(parts[1]) if len(parts) > 1 else 0,
        acquired_at=now.isoformat(),
        expires_at=expires.isoformat(),
        github_user=github_user,
    )


def get_lease_state(
    gh_path: str,
    repo_path: str,
    owner: str,
    repo: str,
    issue_number: int,
) -> tuple[LeaseState | None, str | None]:
    """Get the current lease state for an issue.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        owner: Repository owner.
        repo: Repository name.
        issue_number: The issue number.

    Returns:
        Tuple of (LeaseState, error_message).
    """
    from kagan.core.plugins.github.gh_adapter import (
        run_gh_api_issue_comments,
        run_gh_issue_view,
    )

    # Get issue data with labels
    issue_data, error = run_gh_issue_view(gh_path, repo_path, issue_number)
    if error:
        return None, error

    if issue_data is None:
        return None, "No issue data returned"

    # Check for lease label
    labels_raw = issue_data.get("labels", [])
    labels = [
        label.get("name", "") if isinstance(label, dict) else str(label) for label in labels_raw
    ]
    has_label = LEASE_LABEL in labels

    if not has_label:
        return LeaseState(has_label=False, holder=None, comment_id=None), None

    # Fetch comments via API to get IDs
    comments_raw, error = run_gh_api_issue_comments(gh_path, repo_path, owner, repo, issue_number)
    if error:
        # Fail closed when holder metadata cannot be inspected.
        return LeaseState(
            has_label=True,
            holder=None,
            comment_id=None,
            metadata_complete=False,
        ), None

    # Find lease comment
    holder: LeaseHolder | None = None
    comment_id: int | None = None
    for comment in comments_raw or []:
        body = comment.get("body", "")
        parsed = parse_lease_comment(body)
        if parsed is not None:
            holder = parsed
            comment_id = comment.get("id")
            # Prefer the most recent lease comment when multiple exist.
            continue

    return LeaseState(has_label=has_label, holder=holder, comment_id=comment_id), None


def acquire_lease(
    gh_path: str,
    repo_path: str,
    owner: str,
    repo: str,
    issue_number: int,
    *,
    github_user: str | None = None,
    force_takeover: bool = False,
) -> LeaseAcquireResult:
    """Attempt to acquire a lease on an issue.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        owner: Repository owner.
        repo: Repository name.
        issue_number: The issue number.
        github_user: Optional GitHub username for attribution.
        force_takeover: If True, take over even if another instance holds the lease.

    Returns:
        LeaseAcquireResult indicating success or failure.
    """
    from kagan.core.plugins.github.gh_adapter import (
        run_gh_api_comment_delete,
        run_gh_issue_comment_create,
        run_gh_issue_label_add,
        run_gh_issue_label_remove,
    )

    # Check current state
    state, error = get_lease_state(gh_path, repo_path, owner, repo, issue_number)
    if error:
        return LeaseAcquireResult.failed(f"Failed to check lease state: {error}")

    if state is None:
        return LeaseAcquireResult.failed("Failed to get lease state")

    # Check if we can acquire
    if state.is_held_by_current_instance:
        # We already hold the lease - renew it
        new_holder = create_lease_holder(github_user=github_user)

        # Create new lease comment
        comment_body = build_lease_comment_body(new_holder)
        _, error = run_gh_issue_comment_create(gh_path, repo_path, issue_number, comment_body)
        if error:
            return LeaseAcquireResult.failed(f"Failed to create lease comment: {error}")

        # Best-effort cleanup after renewal succeeds.
        if state.comment_id:
            run_gh_api_comment_delete(gh_path, repo_path, owner, repo, state.comment_id)

        return LeaseAcquireResult.renewed(new_holder)

    if state.requires_takeover and not force_takeover:
        # Another instance holds the lease
        if state.holder is not None:
            return LeaseAcquireResult.blocked(state.holder)
        return LeaseAcquireResult.takeover_required(
            "Lease label is present but holder metadata could not be verified. "
            "Retry, or use force_takeover=true to proceed."
        )

    # Can acquire (either free, stale, or force takeover)
    new_holder = create_lease_holder(github_user=github_user)

    # Add label if not present
    label_added = False
    if not state.has_label:
        success, error = run_gh_issue_label_add(gh_path, repo_path, issue_number, LEASE_LABEL)
        if not success:
            return LeaseAcquireResult.failed(f"Failed to add lease label: {error}")
        label_added = True

    # Create lease comment
    comment_body = build_lease_comment_body(new_holder)
    _, error = run_gh_issue_comment_create(gh_path, repo_path, issue_number, comment_body)
    if error:
        if label_added:
            run_gh_issue_label_remove(gh_path, repo_path, issue_number, LEASE_LABEL)
        return LeaseAcquireResult.failed(f"Failed to create lease comment: {error}")

    # Best-effort cleanup once we have fresh metadata.
    if state.comment_id:
        run_gh_api_comment_delete(gh_path, repo_path, owner, repo, state.comment_id)

    return LeaseAcquireResult.acquired(new_holder)


def release_lease(
    gh_path: str,
    repo_path: str,
    owner: str,
    repo: str,
    issue_number: int,
) -> LeaseReleaseResult:
    """Release a lease on an issue.

    Only succeeds if the current instance holds the lease.

    Args:
        gh_path: Path to gh CLI.
        repo_path: Path to repository directory.
        owner: Repository owner.
        repo: Repository name.
        issue_number: The issue number.

    Returns:
        LeaseReleaseResult indicating success or failure.
    """
    from kagan.core.plugins.github.gh_adapter import (
        run_gh_api_comment_delete,
        run_gh_issue_label_remove,
    )

    # Check current state
    state, error = get_lease_state(gh_path, repo_path, owner, repo, issue_number)
    if error:
        return LeaseReleaseResult.failed(f"Failed to check lease state: {error}")

    if state is None:
        return LeaseReleaseResult.failed("Failed to get lease state")

    if state.has_label and not state.metadata_complete:
        return LeaseReleaseResult.failed(
            "Cannot release lease because holder metadata could not be verified. "
            "Retry when GitHub comments are accessible."
        )

    # Check if we hold the lease
    if not state.is_held_by_current_instance:
        return LeaseReleaseResult.not_held()

    # Remove label
    success, error = run_gh_issue_label_remove(gh_path, repo_path, issue_number, LEASE_LABEL)
    if not success:
        return LeaseReleaseResult.failed(f"Failed to remove lease label: {error}")

    # Delete lease comment
    if state.comment_id:
        run_gh_api_comment_delete(gh_path, repo_path, owner, repo, state.comment_id)

    return LeaseReleaseResult.released()


__all__ = [
    "LEASE_ACQUIRE_FAILED",
    "LEASE_COMMENT_MARKER",
    "LEASE_DURATION_SECONDS",
    "LEASE_HELD_BY_OTHER",
    "LEASE_LABEL",
    "LEASE_NOT_HELD",
    "LEASE_RELEASE_FAILED",
    "LEASE_STALE_THRESHOLD_SECONDS",
    "LeaseAcquireResult",
    "LeaseHolder",
    "LeaseReleaseResult",
    "LeaseState",
    "acquire_lease",
    "build_lease_comment_body",
    "create_lease_holder",
    "get_lease_state",
    "parse_lease_comment",
    "release_lease",
]
