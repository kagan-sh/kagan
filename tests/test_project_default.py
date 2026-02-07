"""Regression tests for project defaults."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from kagan.bootstrap import create_app_context
from kagan.config import KaganConfig

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.asyncio
async def test_open_project_sets_default_project_for_task_creation(tmp_path: Path) -> None:
    config_path = tmp_path / "config.toml"
    db_path = tmp_path / "kagan.db"

    ctx = await create_app_context(
        config_path,
        db_path,
        config=KaganConfig(),
        project_root=tmp_path,
    )
    try:
        project_id = await ctx.project_service.create_project(
            name="Default Project",
            repo_paths=[tmp_path],
        )
        await ctx.project_service.open_project(project_id)

        task = await ctx.task_service.create_task(
            title="Created without explicit project_id",
            description="",
        )
        assert task.project_id == project_id
    finally:
        await ctx.close()
