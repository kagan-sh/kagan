from __future__ import annotations

import asyncio
import time
from typing import TYPE_CHECKING
from unittest.mock import patch

import pytest

from kagan.core.ipc.discovery import CoreEndpoint
from kagan.core.launcher import ensure_core_running

if TYPE_CHECKING:
    from pathlib import Path


class _DummyProcess:
    def poll(self) -> None:
        return None


class _ExitedProcess:
    def poll(self) -> int:
        return 1


@pytest.mark.asyncio
async def test_ensure_core_running_spawns_once_for_concurrent_callers(tmp_path: Path) -> None:
    endpoint = CoreEndpoint(transport="socket", address="/tmp/kagan-core.sock")
    state = {"spawn_count": 0, "started_at": 0.0}

    def _discover(**_kwargs: object) -> CoreEndpoint | None:
        if state["spawn_count"] == 0:
            return None
        if time.monotonic() - state["started_at"] < 0.03:
            return None
        return endpoint

    def _spawn_core_detached(**_kwargs: object) -> _DummyProcess:
        state["spawn_count"] += 1
        state["started_at"] = time.monotonic()
        return _DummyProcess()

    with (
        patch("kagan.core.launcher.discover_core_endpoint", side_effect=_discover),
        patch("kagan.core.launcher._spawn_core_detached", side_effect=_spawn_core_detached),
        patch("kagan.core.launcher._CORE_START_POLL_SECONDS", 0.01),
        patch.dict(
            "os.environ",
            {"KAGAN_CORE_RUNTIME_DIR": str(tmp_path / "core-runtime")},
            clear=False,
        ),
    ):
        endpoint_one, endpoint_two = await asyncio.gather(
            ensure_core_running(timeout=0.3),
            ensure_core_running(timeout=0.3),
        )

    assert endpoint_one == endpoint
    assert endpoint_two == endpoint
    assert state["spawn_count"] == 1


@pytest.mark.asyncio
async def test_ensure_core_running_waits_when_parallel_launcher_wins(tmp_path: Path) -> None:
    endpoint = CoreEndpoint(transport="socket", address="/tmp/kagan-core.sock")
    state = {"calls": 0}

    def _discover(**_kwargs: object) -> CoreEndpoint | None:
        state["calls"] += 1
        if state["calls"] < 4:
            return None
        return endpoint

    with (
        patch("kagan.core.launcher.discover_core_endpoint", side_effect=_discover),
        patch("kagan.core.launcher._spawn_core_detached", return_value=_ExitedProcess()),
        patch("kagan.core.launcher._has_live_core_instance_lock", return_value=True),
        patch("kagan.core.launcher._CORE_START_POLL_SECONDS", 0.01),
        patch.dict(
            "os.environ",
            {"KAGAN_CORE_RUNTIME_DIR": str(tmp_path / "core-runtime")},
            clear=False,
        ),
    ):
        resolved = await ensure_core_running(timeout=0.3)

    assert resolved == endpoint
