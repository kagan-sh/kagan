"""Focused tests for generic plugin API dispatch (formerly typed GitHub API methods)."""

from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock

import pytest
from _api_helpers import build_api

if TYPE_CHECKING:
    from collections.abc import AsyncGenerator
    from pathlib import Path

    from kagan.core.api import KaganAPI
    from kagan.core.bootstrap import AppContext


@pytest.fixture
async def github_api_env(
    tmp_path: Path,
) -> AsyncGenerator[tuple[KaganAPI, AppContext]]:
    repo, api, ctx = await build_api(tmp_path)
    yield api, ctx
    await repo.close()


async def test_invoke_plugin_forwards_params_to_handler(
    github_api_env: tuple[KaganAPI, AppContext],
) -> None:
    api, ctx = github_api_env
    handler = AsyncMock(return_value={"success": True, "stats": {"inserted": 3}})
    registry = MagicMock()
    registry.resolve_operation.return_value = SimpleNamespace(handler=handler)
    ctx.plugin_registry = registry

    result = await api.invoke_plugin(
        "kagan_github", "sync_issues", {"project_id": "project-1", "repo_id": "repo-1"}
    )

    assert result["success"] is True
    assert result["stats"]["inserted"] == 3
    handler.assert_awaited_once_with(
        ctx,
        {
            "project_id": "project-1",
            "repo_id": "repo-1",
        },
    )


async def test_invoke_plugin_raises_when_registry_missing(
    github_api_env: tuple[KaganAPI, AppContext],
) -> None:
    api, ctx = github_api_env
    # Ensure no registry
    ctx.plugin_registry = None  # type: ignore[attr-defined]

    with pytest.raises(RuntimeError, match="Plugin registry is not initialized"):
        await api.invoke_plugin("kagan_github", "connect_repo", {"project_id": "p1"})
