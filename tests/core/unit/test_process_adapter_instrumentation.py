from __future__ import annotations

from unittest.mock import AsyncMock

import pytest

from kagan.core.adapters.process import run_exec_capture
from kagan.core.instrumentation import configure, reset, snapshot


class _ProcessStub:
    def __init__(self, *, returncode: int) -> None:
        self.returncode = returncode


async def test_run_exec_capture_records_counter_and_timing(monkeypatch: pytest.MonkeyPatch) -> None:
    previous = snapshot()
    try:
        configure(enabled=True, log_events=False)
        reset()

        monkeypatch.setattr(
            "kagan.core.adapters.process.spawn_exec",
            AsyncMock(return_value=_ProcessStub(returncode=0)),
        )
        monkeypatch.setattr(
            "kagan.core.adapters.process._communicate",
            AsyncMock(return_value=(b"stdout", b"stderr")),
        )

        result = await run_exec_capture("git", "status")
        assert result.returncode == 0
        assert result.stdout == b"stdout"
        assert result.stderr == b"stderr"

        state = snapshot()
        assert state["counters"]["core.process.exec.calls"] == 1
        assert state["timings"]["core.process.exec.duration_ms"]["count"] == 1
        assert "core.process.exec.nonzero_returncode" not in state["counters"]
    finally:
        configure(
            enabled=bool(previous["enabled"]),
            log_events=bool(previous["log_events"]),
        )
        reset()


async def test_run_exec_capture_records_timeout_counter(monkeypatch: pytest.MonkeyPatch) -> None:
    previous = snapshot()
    try:
        configure(enabled=True, log_events=False)
        reset()

        monkeypatch.setattr(
            "kagan.core.adapters.process.spawn_exec",
            AsyncMock(return_value=_ProcessStub(returncode=1)),
        )
        monkeypatch.setattr(
            "kagan.core.adapters.process._communicate",
            AsyncMock(side_effect=TimeoutError),
        )

        with pytest.raises(TimeoutError):
            await run_exec_capture("git", "status")

        state = snapshot()
        assert state["counters"]["core.process.exec.calls"] == 1
        assert state["counters"]["core.process.exec.timeouts"] == 1
        assert state["timings"]["core.process.exec.duration_ms"]["count"] == 1
    finally:
        configure(
            enabled=bool(previous["enabled"]),
            log_events=bool(previous["log_events"]),
        )
        reset()
