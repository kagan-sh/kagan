"""Tests for diagnostics instrumentation api adapter (formerly CQRS handler)."""

from __future__ import annotations

from types import SimpleNamespace
from typing import Any, cast

from kagan.core.api import KaganAPI
from kagan.core.request_handlers import handle_diagnostics_instrumentation


async def test_instrumentation_snapshot_returns_core_state(monkeypatch) -> None:
    sentinel = {
        "enabled": True,
        "log_events": False,
        "counters": {"core.process.exec.calls": 3},
        "timings": {"core.process.exec.duration_ms": {"count": 3}},
    }
    monkeypatch.setattr(
        "kagan.core.api.instrumentation_snapshot",
        lambda: sentinel,
    )

    ctx = SimpleNamespace()
    f = KaganAPI(cast("Any", ctx))
    result = await handle_diagnostics_instrumentation(f, {})

    assert result["instrumentation"] == sentinel
