from __future__ import annotations

from contextlib import contextmanager
from typing import TYPE_CHECKING, Any

from kagan.core.instrumentation import (
    configure,
    increment_counter,
    reset,
    snapshot,
    timed_operation,
)

if TYPE_CHECKING:
    from collections.abc import Iterator


@contextmanager
def _preserve_runtime_flags() -> Iterator[dict[str, Any]]:
    previous = snapshot()
    try:
        yield previous
    finally:
        configure(
            enabled=bool(previous["enabled"]),
            log_events=bool(previous["log_events"]),
        )
        reset()


def test_instrumentation_is_noop_when_disabled() -> None:
    with _preserve_runtime_flags():
        configure(enabled=False, log_events=False)
        reset()

        increment_counter("core.test.counter")
        with timed_operation("core.test.timer"):
            pass

        state = snapshot()
        assert state["counters"] == {}
        assert state["timings"] == {}


def test_instrumentation_tracks_counter_and_timing_when_enabled() -> None:
    with _preserve_runtime_flags():
        configure(enabled=True, log_events=False)
        reset()

        increment_counter("core.test.counter")
        increment_counter("core.test.counter", amount=2)
        with timed_operation("core.test.timer"):
            pass

        state = snapshot()
        assert state["counters"]["core.test.counter"] == 3
        assert state["timings"]["core.test.timer"]["count"] == 1
        assert state["timings"]["core.test.timer"]["total_ms"] >= 0.0
