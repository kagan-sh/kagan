"""Shared fixtures for core unit tests."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest
from _api_helpers import build_api

if TYPE_CHECKING:
    from collections.abc import AsyncGenerator
    from pathlib import Path

    from kagan.core.adapters.db.repositories import TaskRepository
    from kagan.core.api import KaganAPI
    from kagan.core.bootstrap import AppContext


@pytest.fixture
async def api_env(
    tmp_path: Path,
) -> AsyncGenerator[tuple[TaskRepository, KaganAPI, AppContext]]:
    """Build API environment with real task/project services and mocked externals."""
    task_repo, api, ctx = await build_api(tmp_path)
    yield task_repo, api, ctx
    await task_repo.close()
