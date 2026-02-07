"""Project service interface and implementation."""

from __future__ import annotations

from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, Protocol

from sqlmodel import col, select

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

    from kagan.adapters.db.repositories import RepoRepository
    from kagan.adapters.db.schema import Project, Repo
    from kagan.core.events import EventBus
    from kagan.services.types import ProjectId, RepoId


class ProjectService(Protocol):
    """Service interface for project operations."""

    async def create_project(
        self,
        name: str,
        repo_paths: list[str | Path] | None = None,
        description: str | None = None,
    ) -> str:
        """Create a project with repos, return project_id."""
        ...

    async def open_project(self, project_id: ProjectId) -> Project:
        """Open project (update last_opened_at, publish ProjectOpened event)."""
        ...

    async def get_project(self, project_id: ProjectId) -> Project | None:
        """Return a project by ID."""
        ...

    async def list_recent_projects(self, limit: int = 10) -> list[Project]:
        """Get recently opened projects sorted by last_opened_at desc."""
        ...

    async def add_repo_to_project(
        self,
        project_id: ProjectId,
        repo_path: str | Path,
        is_primary: bool = False,
    ) -> str:
        """Add repo to project, return repo_id."""
        ...

    async def remove_repo_from_project(
        self,
        project_id: ProjectId,
        repo_id: RepoId,
    ) -> None:
        """Remove repo from project."""
        ...

    async def get_project_repos(self, project_id: ProjectId) -> list[Repo]:
        """Get all repos for a project."""
        ...

    async def get_project_repo_details(self, project_id: ProjectId) -> list[dict]:
        """Get all repos for a project with junction metadata."""
        ...

    async def find_project_by_repo_path(self, repo_path: str | Path) -> Project | None:
        """Find project containing the repo."""
        ...


class ProjectServiceImpl:
    """Concrete ProjectService backed by session factory, EventBus, and RepoRepository."""

    def __init__(
        self,
        session_factory: async_sessionmaker[AsyncSession],
        event_bus: EventBus,
        repo_repository: RepoRepository,
    ) -> None:
        self._session_factory = session_factory
        self._events = event_bus
        self._repo_repository = repo_repository

    def _get_session(self) -> AsyncSession:
        """Get a new async session."""
        return self._session_factory()

    async def create_project(
        self,
        name: str,
        repo_paths: list[str | Path] | None = None,
        description: str | None = None,
    ) -> str:
        """Create a project with repos, return project_id."""
        from kagan.adapters.db.schema import Project as DbProject
        from kagan.core.events import ProjectCreated

        repo_paths = repo_paths or []

        async with self._get_session() as session:
            project = DbProject(
                name=name,
                description=description or "",
                last_opened_at=datetime.now(),
            )
            session.add(project)
            await session.commit()
            await session.refresh(project)

            project_id = project.id

        for i, repo_path in enumerate(repo_paths):
            is_primary = i == 0
            await self.add_repo_to_project(project_id, repo_path, is_primary=is_primary)

        await self._events.publish(
            ProjectCreated(
                project_id=project_id,
                name=name,
                repo_count=len(repo_paths),
            )
        )

        return project_id

    async def open_project(self, project_id: ProjectId) -> Project:
        """Open project (update last_opened_at, publish ProjectOpened event)."""
        from kagan.adapters.db.schema import Project as DbProject
        from kagan.core.events import ProjectOpened

        async with self._get_session() as session:
            project = await session.get(DbProject, project_id)
            if project is None:
                raise ValueError(f"Project not found: {project_id}")

            project.last_opened_at = datetime.now()
            project.updated_at = datetime.now()
            session.add(project)
            await session.commit()
            await session.refresh(project)

            await self._events.publish(ProjectOpened(project_id=project_id))

            return project

    async def get_project(self, project_id: ProjectId) -> Project | None:
        """Return a project by ID."""
        from kagan.adapters.db.schema import Project as DbProject

        async with self._get_session() as session:
            return await session.get(DbProject, project_id)

    async def list_recent_projects(self, limit: int = 10) -> list[Project]:
        """Get recently opened projects sorted by last_opened_at desc."""
        from kagan.adapters.db.schema import Project as DbProject

        async with self._get_session() as session:
            result = await session.execute(
                select(DbProject)
                .where(col(DbProject.last_opened_at).is_not(None))
                .order_by(col(DbProject.last_opened_at).desc())
                .limit(limit)
            )
            return list(result.scalars().all())

    async def add_repo_to_project(
        self,
        project_id: ProjectId,
        repo_path: str | Path,
        is_primary: bool = False,
    ) -> str:
        """Add repo to project, return repo_id."""
        repo, _ = await self._repo_repository.get_or_create(path=repo_path)

        existing_repos = await self._repo_repository.list_for_project(project_id)
        display_order = len(existing_repos)

        await self._repo_repository.add_to_project(
            project_id=project_id,
            repo_id=repo.id,
            is_primary=is_primary,
            display_order=display_order,
        )

        return repo.id

    async def remove_repo_from_project(
        self,
        project_id: ProjectId,
        repo_id: RepoId,
    ) -> None:
        """Remove repo from project."""
        await self._repo_repository.remove_from_project(project_id, repo_id)

    async def get_project_repos(self, project_id: ProjectId) -> list[Repo]:
        """Get all repos for a project."""
        return await self._repo_repository.list_for_project(project_id)

    async def get_project_repo_details(self, project_id: ProjectId) -> list[dict]:
        """Get all repos for a project with junction metadata."""
        from kagan.adapters.db.schema import ProjectRepo, Repo

        async with self._get_session() as session:
            result = await session.execute(
                select(ProjectRepo, Repo)
                .join(Repo)
                .where(ProjectRepo.project_id == project_id)
                .order_by(col(ProjectRepo.display_order))
            )
            return [
                {
                    "id": repo.id,
                    "name": repo.name,
                    "path": repo.path,
                    "default_branch": repo.default_branch,
                    "is_primary": project_repo.is_primary,
                    "display_order": project_repo.display_order,
                }
                for project_repo, repo in result.all()
            ]

    async def find_project_by_repo_path(self, repo_path: str | Path) -> Project | None:
        """Find project containing the repo."""
        from kagan.adapters.db.schema import Project as DbProject
        from kagan.adapters.db.schema import ProjectRepo, Repo

        resolved_path = str(Path(repo_path).resolve())

        async with self._get_session() as session:
            result = await session.execute(select(Repo).where(Repo.path == resolved_path))
            repo = result.scalars().first()

            if repo is None:
                return None

            result = await session.execute(
                select(ProjectRepo).where(ProjectRepo.repo_id == repo.id)
            )
            project_repo = result.scalars().first()

            if project_repo is None:
                return None

            return await session.get(DbProject, project_repo.project_id)
