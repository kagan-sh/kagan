from __future__ import annotations

from unittest.mock import AsyncMock

from kagan.core.adapters.process import ProcessResult
from kagan.core.git_utils import _run_git
from kagan.core.instrumentation import configure, reset, snapshot


async def test_run_git_records_counters_and_timing(monkeypatch) -> None:
    previous = snapshot()
    try:
        configure(enabled=True, log_events=False)
        reset()

        monkeypatch.setattr(
            "kagan.core.git_utils.run_exec_capture",
            AsyncMock(return_value=ProcessResult(returncode=1, stdout=b"", stderr=b"err")),
        )

        await _run_git("status")

        state = snapshot()
        assert state["counters"]["core.git.command.calls"] == 1
        assert state["counters"]["core.git.command.nonzero_returncode"] == 1
        assert state["timings"]["core.git.command.duration_ms"]["count"] == 1
    finally:
        configure(
            enabled=bool(previous["enabled"]),
            log_events=bool(previous["log_events"]),
        )
        reset()
