"""Lightweight optional counters and timings for core hotspots."""

from __future__ import annotations

import json
import logging
import os
import threading
import time
from contextlib import contextmanager
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Iterator

logger = logging.getLogger(__name__)

_ENABLED_VALUES = frozenset({"1", "true", "yes", "on"})
_INSTRUMENTATION_ENV = "KAGAN_CORE_INSTRUMENTATION"
_INSTRUMENTATION_LOG_ENV = "KAGAN_CORE_INSTRUMENTATION_LOG"


def _is_env_enabled(name: str) -> bool:
    raw = os.environ.get(name)
    if raw is None:
        return False
    return raw.strip().lower() in _ENABLED_VALUES


@dataclass(slots=True)
class _TimingStats:
    count: int = 0
    total_ms: float = 0.0
    min_ms: float = float("inf")
    max_ms: float = 0.0

    def add(self, duration_ms: float) -> None:
        self.count += 1
        self.total_ms += duration_ms
        if duration_ms < self.min_ms:
            self.min_ms = duration_ms
        if duration_ms > self.max_ms:
            self.max_ms = duration_ms

    def to_dict(self) -> dict[str, float | int]:
        min_ms = 0.0 if self.min_ms == float("inf") else self.min_ms
        avg_ms = self.total_ms / self.count if self.count else 0.0
        return {
            "count": self.count,
            "total_ms": self.total_ms,
            "avg_ms": avg_ms,
            "min_ms": min_ms,
            "max_ms": self.max_ms,
        }


_lock = threading.Lock()
_enabled = _is_env_enabled(_INSTRUMENTATION_ENV)
_log_events = _is_env_enabled(_INSTRUMENTATION_LOG_ENV)
_counters: dict[str, int] = {}
_timings: dict[str, _TimingStats] = {}


def configure(*, enabled: bool | None = None, log_events: bool | None = None) -> None:
    """Update runtime instrumentation flags."""
    global _enabled, _log_events
    if enabled is not None:
        _enabled = enabled
    if log_events is not None:
        _log_events = log_events


def is_enabled() -> bool:
    """Return whether instrumentation is currently enabled."""
    return _enabled


def reset() -> None:
    """Clear all in-memory counters and timings."""
    with _lock:
        _counters.clear()
        _timings.clear()


def snapshot() -> dict[str, Any]:
    """Return a copy of current instrumentation aggregates."""
    with _lock:
        counters = dict(_counters)
        timings = {name: stats.to_dict() for name, stats in _timings.items()}
    return {
        "enabled": _enabled,
        "log_events": _log_events,
        "counters": counters,
        "timings": timings,
    }


def _emit_structured_event(
    *,
    kind: str,
    name: str,
    value: float | int,
    fields: dict[str, Any] | None = None,
) -> None:
    if not _log_events:
        return
    payload: dict[str, Any] = {
        "kind": kind,
        "name": name,
        "value": value,
    }
    if fields:
        payload["fields"] = fields
    logger.info("core.instrumentation %s", json.dumps(payload, sort_keys=True, default=str))


def increment_counter(
    name: str,
    *,
    amount: int = 1,
    fields: dict[str, Any] | None = None,
) -> None:
    """Increment a named counter if instrumentation is enabled."""
    if not _enabled:
        return
    with _lock:
        _counters[name] = _counters.get(name, 0) + amount
    _emit_structured_event(kind="counter", name=name, value=amount, fields=fields)


def record_timing(
    name: str,
    duration_ms: float,
    *,
    fields: dict[str, Any] | None = None,
) -> None:
    """Record an elapsed duration in milliseconds."""
    if not _enabled:
        return
    with _lock:
        stats = _timings.get(name)
        if stats is None:
            stats = _TimingStats()
            _timings[name] = stats
        stats.add(duration_ms)
    _emit_structured_event(kind="timing", name=name, value=duration_ms, fields=fields)


@contextmanager
def timed_operation(
    name: str,
    *,
    fields: dict[str, Any] | None = None,
) -> Iterator[None]:
    """Measure a block and record timing if enabled."""
    if not _enabled:
        yield
        return
    started_at = time.perf_counter()
    try:
        yield
    finally:
        elapsed_ms = (time.perf_counter() - started_at) * 1000.0
        record_timing(name, elapsed_ms, fields=fields)


__all__ = [
    "configure",
    "increment_counter",
    "is_enabled",
    "record_timing",
    "reset",
    "snapshot",
    "timed_operation",
]
