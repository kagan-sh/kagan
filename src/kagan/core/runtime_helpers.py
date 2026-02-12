"""Core runtime helpers: idempotency and runtime snapshot serialization."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from typing import TYPE_CHECKING, Any, Protocol, TypedDict

if TYPE_CHECKING:
    import asyncio

# ---------------------------------------------------------------------------
# Idempotency
# ---------------------------------------------------------------------------

IDEMPOTENCY_CACHE_LIMIT = 512

IDEMPOTENT_MUTATION_METHODS: set[tuple[str, str]] = {
    ("tasks", "create"),
    ("tasks", "update"),
    ("tasks", "move"),
    ("tasks", "delete"),
    ("tasks", "update_scratchpad"),
    ("review", "request"),
    ("review", "approve"),
    ("review", "reject"),
    ("review", "merge"),
    ("review", "rebase"),
    ("jobs", "submit"),
    ("jobs", "cancel"),
    ("sessions", "create"),
    ("sessions", "attach"),
    ("sessions", "kill"),
    ("projects", "create"),
    ("projects", "open"),
    ("projects", "add_repo"),
    ("settings", "update"),
}


@dataclass(frozen=True, slots=True)
class CachedResponseEnvelope:
    ok: bool
    result: dict[str, Any] | None
    error_code: str | None
    error_message: str | None


@dataclass(slots=True)
class IdempotencyRecord:
    fingerprint: str
    response: CachedResponseEnvelope | None = None
    pending: asyncio.Future[CachedResponseEnvelope] | None = None


@dataclass(frozen=True, slots=True)
class IdempotencyReservation:
    cache_key: tuple[str, str]
    fingerprint: str
    pending: asyncio.Future[CachedResponseEnvelope]
    owner: bool


# ---------------------------------------------------------------------------
# Runtime snapshot
# ---------------------------------------------------------------------------


class RuntimeSnapshot(TypedDict):
    """Serialized runtime state exposed to query/command callers."""

    is_running: bool
    is_reviewing: bool
    is_blocked: bool
    blocked_reason: str | None
    blocked_by_task_ids: list[str]
    overlap_hints: list[str]
    blocked_at: str | None
    is_pending: bool
    pending_reason: str | None
    pending_at: str | None


class RuntimeSnapshotSource(Protocol):
    """Minimal runtime service interface required for snapshot lookup."""

    def get(self, task_id: str) -> object | None:
        """Return runtime view for task_id or None when unavailable."""


def empty_runtime_snapshot() -> RuntimeSnapshot:
    """Return default runtime payload when there is no active runtime view."""
    return RuntimeSnapshot(
        is_running=False,
        is_reviewing=False,
        is_blocked=False,
        blocked_reason=None,
        blocked_by_task_ids=[],
        overlap_hints=[],
        blocked_at=None,
        is_pending=False,
        pending_reason=None,
        pending_at=None,
    )


def _iso_or_none(value: object) -> str | None:
    if isinstance(value, datetime):
        return value.isoformat()
    if isinstance(value, str):
        return value
    return None


def serialize_runtime_view(view: object | None) -> RuntimeSnapshot:
    """Serialize runtime view object into a stable dict payload."""
    if view is None:
        return empty_runtime_snapshot()
    return RuntimeSnapshot(
        is_running=bool(getattr(view, "is_running", False)),
        is_reviewing=bool(getattr(view, "is_reviewing", False)),
        is_blocked=bool(getattr(view, "is_blocked", False)),
        blocked_reason=getattr(view, "blocked_reason", None),
        blocked_by_task_ids=[str(task_id) for task_id in getattr(view, "blocked_by_task_ids", ())],
        overlap_hints=[str(hint) for hint in getattr(view, "overlap_hints", ())],
        blocked_at=_iso_or_none(getattr(view, "blocked_at", None)),
        is_pending=bool(getattr(view, "is_pending", False)),
        pending_reason=getattr(view, "pending_reason", None),
        pending_at=_iso_or_none(getattr(view, "pending_at", None)),
    )


def runtime_snapshot_for_task(
    *,
    task_id: str,
    runtime_service: RuntimeSnapshotSource | None,
) -> RuntimeSnapshot:
    """Resolve task runtime snapshot from a runtime service if available."""
    if runtime_service is None:
        return empty_runtime_snapshot()
    return serialize_runtime_view(runtime_service.get(task_id))


__all__ = [
    "IDEMPOTENCY_CACHE_LIMIT",
    "IDEMPOTENT_MUTATION_METHODS",
    "CachedResponseEnvelope",
    "IdempotencyRecord",
    "IdempotencyReservation",
    "RuntimeSnapshot",
    "RuntimeSnapshotSource",
    "empty_runtime_snapshot",
    "runtime_snapshot_for_task",
    "serialize_runtime_view",
]
