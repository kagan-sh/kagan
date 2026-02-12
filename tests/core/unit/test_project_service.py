from __future__ import annotations

from datetime import datetime, timedelta
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock

import pytest
from sqlalchemy import text
from sqlmodel import select

from kagan.core.adapters.db.repositories import RepoRepository, TaskRepository
from kagan.core.adapters.db.schema import Project
from kagan.core.bootstrap import InMemoryEventBus
from kagan.core.services.projects import ProjectServiceImpl

if TYPE_CHECKING:
    from pathlib import Path


async def _build_project_service(db_path: Path) -> tuple[TaskRepository, ProjectServiceImpl]:
    repo = TaskRepository(db_path)
    await repo.initialize()
    if repo._session_factory is None:  # pragma: no cover
        raise RuntimeError("Session factory not initialized")

    service = ProjectServiceImpl(
        session_factory=repo._session_factory,
        event_bus=InMemoryEventBus(),
        repo_repository=RepoRepository(repo._session_factory),
    )
    return repo, service


async def _set_project_last_opened(
    repo: TaskRepository,
    project_id: str,
    opened_at: datetime | None,
) -> None:
    if repo._session_factory is None:  # pragma: no cover
        raise RuntimeError("Session factory not initialized")

    async with repo._session_factory() as session:
        project = await session.get(Project, project_id)
        if project is None:  # pragma: no cover
            raise RuntimeError(f"Project not found: {project_id}")
        project.last_opened_at = opened_at
        project.updated_at = datetime.now()
        session.add(project)
        await session.commit()


async def test_find_project_by_repo_path_prefers_most_specific_containing_repo(
    tmp_path: Path,
) -> None:
    db_path = tmp_path / "kagan.db"
    repo, service = await _build_project_service(db_path)

    try:
        parent_repo = tmp_path / "workspace" / "parent-repo"
        nested_repo = parent_repo / "tools" / "nested-repo"
        launch_path = nested_repo / "src" / "module"
        launch_path.mkdir(parents=True)

        parent_project_id = await service.create_project(
            name="Parent Project",
            repo_paths=[parent_repo],
        )
        nested_project_id = await service.create_project(
            name="Nested Project",
            repo_paths=[nested_repo],
        )

        project = await service.find_project_by_repo_path(launch_path)

        assert project is not None
        assert project.id == nested_project_id
        assert project.id != parent_project_id
    finally:
        await repo.close()


async def test_list_recent_projects_prioritizes_recent_created_when_never_opened(
    tmp_path: Path,
) -> None:
    db_path = tmp_path / "kagan.db"
    repo, service = await _build_project_service(db_path)

    try:
        old_repo = tmp_path / "repos" / "old"
        new_repo = tmp_path / "repos" / "new"
        never_repo = tmp_path / "repos" / "never"
        old_repo.mkdir(parents=True)
        new_repo.mkdir(parents=True)
        never_repo.mkdir(parents=True)

        old_project_id = await service.create_project("Opened Old", [old_repo])
        new_project_id = await service.create_project("Opened New", [new_repo])
        never_project_id = await service.create_project("Never Opened", [never_repo])

        now = datetime.now()
        await _set_project_last_opened(repo, old_project_id, now - timedelta(days=2))
        await _set_project_last_opened(repo, new_project_id, now - timedelta(days=1))
        await _set_project_last_opened(repo, never_project_id, None)

        projects = await service.list_recent_projects(limit=10)
        project_ids = [project.id for project in projects]

        assert never_project_id in project_ids
        assert project_ids.index(never_project_id) < project_ids.index(new_project_id)
        assert project_ids.index(new_project_id) < project_ids.index(old_project_id)
    finally:
        await repo.close()


async def test_create_project_rolls_back_when_repo_link_fails(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    db_path = tmp_path / "kagan.db"
    repo, service = await _build_project_service(db_path)

    try:
        failing_repo = tmp_path / "repos" / "failing"
        failing_repo.mkdir(parents=True)

        monkeypatch.setattr(
            service._repo_repository,
            "add_to_project",
            AsyncMock(side_effect=RuntimeError("repo link failed")),
        )

        with pytest.raises(RuntimeError, match="repo link failed"):
            await service.create_project(
                name="Broken Project",
                repo_paths=[failing_repo],
            )

        if repo._session_factory is None:  # pragma: no cover
            raise RuntimeError("Session factory not initialized")

        async with repo._session_factory() as session:
            result = await session.execute(select(Project).where(Project.name == "Broken Project"))
            assert result.scalars().first() is None
    finally:
        await repo.close()


async def test_create_project_rolls_back_project_and_links_when_second_repo_link_fails(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Project creation must remove partial links when a later repo link fails."""
    db_path = tmp_path / "kagan.db"
    repo, service = await _build_project_service(db_path)

    try:
        first_repo_path = tmp_path / "repos" / "first"
        second_repo_path = tmp_path / "repos" / "second"
        first_repo_path.mkdir(parents=True)
        second_repo_path.mkdir(parents=True)

        original_add_to_project = service._repo_repository.add_to_project
        call_count = 0

        async def _fail_on_second_link(
            project_id: str,
            repo_id: str,
            is_primary: bool = False,
            display_order: int = 0,
        ) -> object:
            nonlocal call_count
            call_count += 1
            if call_count == 2:
                raise RuntimeError("second repo link failed")
            return await original_add_to_project(
                project_id=project_id,
                repo_id=repo_id,
                is_primary=is_primary,
                display_order=display_order,
            )

        monkeypatch.setattr(service._repo_repository, "add_to_project", _fail_on_second_link)

        with pytest.raises(RuntimeError, match="second repo link failed"):
            await service.create_project(
                name="Two Repo Project",
                repo_paths=[first_repo_path, second_repo_path],
            )

        if repo._session_factory is None:  # pragma: no cover
            raise RuntimeError("Session factory not initialized")

        async with repo._session_factory() as session:
            project_result = await session.execute(
                select(Project).where(Project.name == "Two Repo Project")
            )
            project = project_result.scalars().first()
            assert project is None

            link_count = await session.scalar(text("SELECT COUNT(*) FROM project_repos"))
            assert link_count == 0
    finally:
        await repo.close()
