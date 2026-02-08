from __future__ import annotations

from datetime import datetime, timedelta
from typing import TYPE_CHECKING

from kagan.adapters.db.repositories import RepoRepository, TaskRepository
from kagan.adapters.db.schema import Project
from kagan.bootstrap import InMemoryEventBus
from kagan.services.projects import ProjectServiceImpl

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
