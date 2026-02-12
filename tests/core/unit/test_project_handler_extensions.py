"""Tests for project api adapter functions (formerly CQRS handlers)."""

from __future__ import annotations

from types import SimpleNamespace
from typing import Any

from kagan.core.adapters.db.schema import Project
from kagan.core.api import KaganAPI
from kagan.core.request_handlers import (
    handle_project_add_repo,
    handle_project_find_by_repo_path,
    handle_project_repo_details,
)


def _api(**services: object) -> KaganAPI:
    from typing import cast

    ctx = SimpleNamespace(**services)
    return KaganAPI(cast("Any", ctx))


async def test_project_add_repo_returns_repo_id() -> None:
    class _ProjectService:
        async def add_repo_to_project(
            self,
            project_id: str,
            repo_path: str,
            is_primary: bool = False,
        ) -> str:
            assert project_id == "P1"
            assert repo_path == "/tmp/repo"
            assert is_primary is True
            return "R1"

    f = _api(project_service=_ProjectService())
    result = await handle_project_add_repo(
        f,
        {"project_id": "P1", "repo_path": "/tmp/repo", "is_primary": True},
    )

    assert result["success"] is True
    assert result["repo_id"] == "R1"


async def test_project_repo_details_returns_count() -> None:
    class _ProjectService:
        async def get_project_repo_details(self, project_id: str) -> list[dict[str, object]]:
            assert project_id == "P1"
            return [
                {"id": "R1", "path": "/tmp/r1"},
                {"id": "R2", "path": "/tmp/r2"},
            ]

    f = _api(project_service=_ProjectService())
    result = await handle_project_repo_details(f, {"project_id": "P1"})

    assert result["count"] == 2
    assert len(result["repos"]) == 2


async def test_project_find_by_repo_path_handles_missing() -> None:
    class _ProjectService:
        async def find_project_by_repo_path(self, repo_path: str) -> Project | None:
            assert repo_path == "/tmp/missing"
            return None

    f = _api(project_service=_ProjectService())
    result = await handle_project_find_by_repo_path(f, {"repo_path": "/tmp/missing"})

    assert result == {"found": False, "project": None}


async def test_project_find_by_repo_path_returns_project() -> None:
    class _ProjectService:
        async def find_project_by_repo_path(self, repo_path: str) -> Project | None:
            assert repo_path == "/tmp/repo"
            return Project(name="Demo", description="desc")

    f = _api(project_service=_ProjectService())
    result = await handle_project_find_by_repo_path(f, {"repo_path": "/tmp/repo"})

    assert result["found"] is True
    assert result["project"]["name"] == "Demo"
