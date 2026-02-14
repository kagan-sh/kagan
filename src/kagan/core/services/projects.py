"""Project service interface and implementation."""

from __future__ import annotations

import asyncio
import os
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, Protocol

from sqlalchemy import func, literal, or_
from sqlmodel import col, select

from kagan.core.adapters.db.session import get_session
from kagan.core.time import utc_now

if TYPE_CHECKING:
    from kagan.core.adapters.db.repositories import ClosingAwareSessionFactory, RepoRepository
    from kagan.core.adapters.db.schema import Project, Repo
    from kagan.core.events import EventBus
    from kagan.core.services.types import ProjectId, RepoId


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

    async def update_repo_default_branch(
        self,
        repo_id: RepoId,
        branch: str,
        *,
        mark_configured: bool = False,
    ) -> Repo | None:
        """Update Repo.default_branch, optionally marking branch as configured."""
        ...

    async def get_repo_script_value(
        self,
        repo_id: RepoId,
        script_key: str,
    ) -> str | None:
        """Read a script value by key from Repo.scripts."""
        ...

    async def update_repo_script_values(
        self,
        repo_id: RepoId,
        script_updates: dict[str, str],
    ) -> dict[str, str]:
        """Merge script key-values into Repo.scripts and return updated scripts."""
        ...


class ProjectServiceImpl:
    """Concrete ProjectService backed by session factory, EventBus, and RepoRepository."""

    def __init__(
        self,
        session_factory: ClosingAwareSessionFactory,
        event_bus: EventBus,
        repo_repository: RepoRepository,
    ) -> None:
        self._session_factory = session_factory
        self._events = event_bus
        self._repo_repository = repo_repository
        self._repo_scripts_lock = asyncio.Lock()

    async def create_project(
        self,
        name: str,
        repo_paths: list[str | Path] | None = None,
        description: str | None = None,
    ) -> str:
        """Create a project with repos, return project_id."""
        from kagan.core.adapters.db.schema import Project as DbProject
        from kagan.core.events import ProjectCreated

        repo_paths = repo_paths or []

        async with get_session(self._session_factory) as session:
            project = DbProject(
                name=name,
                description=description or "",
                last_opened_at=utc_now(),
            )
            session.add(project)
            await session.commit()
            await session.refresh(project)

            project_id = project.id

        try:
            for i, repo_path in enumerate(repo_paths):
                is_primary = i == 0
                await self.add_repo_to_project(project_id, repo_path, is_primary=is_primary)
        except Exception:
            from kagan.core.adapters.db.schema import ProjectRepo

            async with get_session(self._session_factory) as session:
                links_result = await session.execute(
                    select(ProjectRepo).where(ProjectRepo.project_id == project_id)
                )
                for link in links_result.scalars():
                    await session.delete(link)

                project = await session.get(DbProject, project_id)
                if project is not None:
                    await session.delete(project)

                await session.commit()
            raise

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
        from kagan.core.adapters.db.schema import Project as DbProject
        from kagan.core.events import ProjectOpened

        async with get_session(self._session_factory) as session:
            project = await session.get(DbProject, project_id)
            if project is None:
                raise ValueError(f"Project not found: {project_id}")

            project.last_opened_at = utc_now()
            project.updated_at = utc_now()
            session.add(project)
            await session.commit()
            await session.refresh(project)

            await self._events.publish(ProjectOpened(project_id=project_id))

            return project

    async def get_project(self, project_id: ProjectId) -> Project | None:
        """Return a project by ID."""
        from kagan.core.adapters.db.schema import Project as DbProject

        async with get_session(self._session_factory) as session:
            return await session.get(DbProject, project_id)

    async def list_recent_projects(self, limit: int = 10) -> list[Project]:
        """Get recently opened projects sorted by last_opened_at desc."""
        from kagan.core.adapters.db.schema import Project as DbProject

        async with get_session(self._session_factory) as session:
            result = await session.execute(
                select(DbProject)
                .order_by(
                    func.coalesce(
                        col(DbProject.last_opened_at),
                        col(DbProject.updated_at),
                        col(DbProject.created_at),
                    ).desc(),
                )
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
        from kagan.core.adapters.db.schema import ProjectRepo, Repo

        async with get_session(self._session_factory) as session:
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

    async def update_repo_default_branch(
        self,
        repo_id: str,
        branch: str,
        *,
        mark_configured: bool = False,
    ) -> Repo | None:
        """Update Repo.default_branch, optionally marking branch as configured."""
        return await self._repo_repository.update_default_branch(
            repo_id, branch, mark_configured=mark_configured
        )

    async def get_repo_script_value(
        self,
        repo_id: str,
        script_key: str,
    ) -> str | None:
        """Read a script value by key from Repo.scripts."""
        from kagan.core.adapters.db.schema import Repo

        normalized_key = script_key.strip()
        if not normalized_key:
            raise ValueError("script_key cannot be empty")

        async with get_session(self._session_factory) as session:
            repo = await session.get(Repo, repo_id)
            if repo is None:
                raise ValueError(f"Repo not found: {repo_id}")
            if not repo.scripts:
                return None
            return repo.scripts.get(normalized_key)

    async def update_repo_script_values(
        self,
        repo_id: str,
        script_updates: dict[str, str],
    ) -> dict[str, str]:
        """Merge script key-values into Repo.scripts and return updated scripts."""
        from kagan.core.adapters.db.schema import Repo

        if not script_updates:
            raise ValueError("script_updates cannot be empty")

        normalized_updates: dict[str, str] = {}
        for key, value in script_updates.items():
            normalized_key = key.strip()
            if not normalized_key:
                raise ValueError("script_updates keys cannot be empty")
            normalized_updates[normalized_key] = value

        async with self._repo_scripts_lock:
            async with get_session(self._session_factory) as session:
                repo = await session.get(Repo, repo_id)
                if repo is None:
                    raise ValueError(f"Repo not found: {repo_id}")

                next_scripts = dict(repo.scripts) if repo.scripts else {}
                for key in sorted(normalized_updates):
                    next_scripts[key] = normalized_updates[key]
                repo.scripts = next_scripts

                session.add(repo)
                await session.commit()
                return next_scripts

    async def find_project_by_repo_path(self, repo_path: str | Path) -> Project | None:
        """Find project containing the repo."""
        from kagan.core.adapters.db.schema import Project as DbProject
        from kagan.core.adapters.db.schema import ProjectRepo, Repo

        resolved = Path(repo_path).resolve()
        resolved_path = str(resolved)
        path_separator = os.sep

        async with get_session(self._session_factory) as session:
            result = await session.execute(
                select(DbProject, Repo.path)
                .join(ProjectRepo, col(ProjectRepo.project_id) == col(DbProject.id))
                .join(Repo, col(Repo.id) == col(ProjectRepo.repo_id))
                .where(
                    or_(
                        col(Repo.path) == resolved_path,
                        literal(resolved_path).like(col(Repo.path) + path_separator + "%"),
                    )
                )
            )
            matches = [
                (project, path)
                for project, path in result.all()
                if resolved == Path(path) or resolved.is_relative_to(Path(path))
            ]
            if not matches:
                return None

            best_project, _ = max(
                matches,
                key=lambda match: (
                    len(Path(match[1]).parts),
                    match[0].last_opened_at or datetime.min,
                    match[0].updated_at,
                ),
            )
            return best_project
