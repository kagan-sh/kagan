"""Repository for ``Repo`` entities and project/workspace links."""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Any

from sqlmodel import col, select

from kagan.adapters.db.schema import ProjectRepo, Repo

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession

    from kagan.adapters.db.repositories.base import ClosingAwareSessionFactory


class RepoRepository:
    """CRUD operations for Repo entities."""

    def __init__(self, session_factory: ClosingAwareSessionFactory) -> None:
        self._session_factory = session_factory

    def _get_session(self) -> AsyncSession:
        """Get a new async session."""
        return self._session_factory()

    async def create(
        self,
        path: str | Path,
        name: str | None = None,
        display_name: str | None = None,
        default_branch: str = "main",
        **kwargs: Any,
    ) -> Repo:
        """Create a new repo entry."""
        resolved_path = Path(path).resolve()
        repo = Repo(
            path=str(resolved_path),
            name=name or resolved_path.name,
            display_name=display_name or resolved_path.name,
            default_branch=default_branch,
            **kwargs,
        )
        async with self._get_session() as session:
            session.add(repo)
            await session.commit()
            await session.refresh(repo)
            return repo

    async def get(self, repo_id: str) -> Repo | None:
        """Get a repo by ID."""
        async with self._get_session() as session:
            return await session.get(Repo, repo_id)

    async def get_by_path(self, path: str | Path) -> Repo | None:
        """Find a repo by its filesystem path."""
        resolved_path = str(Path(path).resolve())
        async with self._get_session() as session:
            result = await session.execute(select(Repo).where(Repo.path == resolved_path))
            return result.scalars().first()

    async def get_or_create(
        self,
        path: str | Path,
        **kwargs: Any,
    ) -> tuple[Repo, bool]:
        """Get existing repo or create new one. Returns (repo, created)."""
        existing = await self.get_by_path(path)
        if existing:
            return existing, False
        return await self.create(path, **kwargs), True

    async def list_for_project(self, project_id: str) -> list[Repo]:
        """List all repos for a project via junction table."""
        async with self._get_session() as session:
            result = await session.execute(
                select(ProjectRepo)
                .where(ProjectRepo.project_id == project_id)
                .order_by(col(ProjectRepo.display_order))
            )
            links = result.scalars().all()
            repos: list[Repo] = []
            for link in links:
                repo = await session.get(Repo, link.repo_id)
                if repo:
                    repos.append(repo)
            return repos

    async def list_for_workspace(self, workspace_id: str) -> list[Any]:
        """List all workspace-repo associations for a workspace."""
        from kagan.adapters.db.schema import WorkspaceRepo

        async with self._get_session() as session:
            result = await session.execute(
                select(WorkspaceRepo).where(WorkspaceRepo.workspace_id == workspace_id)
            )
            return list(result.scalars().all())

    async def add_to_project(
        self,
        project_id: str,
        repo_id: str,
        is_primary: bool = False,
        display_order: int = 0,
    ) -> Any:
        """Add a repo to a project via junction table."""
        async with self._get_session() as session:
            link = ProjectRepo(
                project_id=project_id,
                repo_id=repo_id,
                is_primary=is_primary,
                display_order=display_order,
            )
            session.add(link)
            await session.commit()
            await session.refresh(link)
            return link

    async def add_to_workspace(
        self,
        workspace_id: str,
        repo_id: str,
        target_branch: str,
        worktree_path: str | None = None,
    ) -> Any:
        """Add a repo to a workspace via junction table."""
        from kagan.adapters.db.schema import WorkspaceRepo

        async with self._get_session() as session:
            link = WorkspaceRepo(
                workspace_id=workspace_id,
                repo_id=repo_id,
                target_branch=target_branch,
                worktree_path=worktree_path,
            )
            session.add(link)
            await session.commit()
            await session.refresh(link)
            return link

    async def remove_from_project(self, project_id: str, repo_id: str) -> bool:
        """Remove a repo from a project. Returns True if removed."""
        async with self._get_session() as session:
            result = await session.execute(
                select(ProjectRepo).where(
                    ProjectRepo.project_id == project_id,
                    ProjectRepo.repo_id == repo_id,
                )
            )
            link = result.scalars().first()
            if link:
                await session.delete(link)
                await session.commit()
                return True
            return False

    async def remove_from_workspace(self, workspace_id: str, repo_id: str) -> bool:
        """Remove a repo from a workspace. Returns True if removed."""
        from kagan.adapters.db.schema import WorkspaceRepo

        async with self._get_session() as session:
            result = await session.execute(
                select(WorkspaceRepo).where(
                    WorkspaceRepo.workspace_id == workspace_id,
                    WorkspaceRepo.repo_id == repo_id,
                )
            )
            link = result.scalars().first()
            if link:
                await session.delete(link)
                await session.commit()
                return True
            return False
