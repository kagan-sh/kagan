from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest

from kagan.core.services.workspaces.service import WorkspaceServiceImpl


def _make_service(*, repo_default_branch: str | None) -> WorkspaceServiceImpl:
    task_service = SimpleNamespace(
        get_task=AsyncMock(
            return_value=SimpleNamespace(
                id="task-1",
                project_id="project-1",
            )
        )
    )
    project_service = SimpleNamespace(
        get_project_repos=AsyncMock(
            return_value=[
                SimpleNamespace(
                    id="repo-1",
                    name="repo-1",
                    path="/tmp/repo-1",
                    default_branch=repo_default_branch,
                )
            ]
        )
    )
    return WorkspaceServiceImpl(
        session_factory=SimpleNamespace(),
        git_adapter=SimpleNamespace(),
        task_service=task_service,
        project_service=project_service,
    )


async def test_create_uses_explicit_base_branch_over_repo_default() -> None:
    service = _make_service(repo_default_branch="main")
    service.provision = AsyncMock(return_value="ws-1")  # type: ignore[method-assign]
    service.get_agent_working_dir = AsyncMock(return_value=Path("/tmp/ws-1"))  # type: ignore[method-assign]

    await service.create("task-1", base_branch="codex/feat/github-plugin")

    repos = service.provision.await_args.args[1]
    assert repos[0].target_branch == "codex/feat/github-plugin"


async def test_create_uses_repo_default_when_task_base_branch_absent() -> None:
    service = _make_service(repo_default_branch="develop")
    service.provision = AsyncMock(return_value="ws-1")  # type: ignore[method-assign]
    service.get_agent_working_dir = AsyncMock(return_value=Path("/tmp/ws-1"))  # type: ignore[method-assign]

    await service.create("task-1")

    repos = service.provision.await_args.args[1]
    assert repos[0].target_branch == "develop"


async def test_create_raises_when_base_and_repo_default_absent() -> None:
    service = _make_service(repo_default_branch=None)
    service.provision = AsyncMock(return_value="ws-1")  # type: ignore[method-assign]
    service.get_agent_working_dir = AsyncMock(return_value=Path("/tmp/ws-1"))  # type: ignore[method-assign]

    with pytest.raises(ValueError, match="has no default branch configured"):
        await service.create("task-1")
