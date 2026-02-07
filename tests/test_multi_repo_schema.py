"""Tests for multi-repo schema changes (Phase 2)."""

from __future__ import annotations

import tempfile
from datetime import datetime
from pathlib import Path

import pytest
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker
from sqlmodel import col, select

from kagan.adapters.db.engine import create_db_engine, create_db_tables
from kagan.adapters.db.schema import (
    Merge,
    Project,
    ProjectRepo,
    Repo,
    Task,
    Workspace,
    WorkspaceRepo,
)
from kagan.core.models.enums import TaskStatus, WorkspaceStatus


@pytest.fixture
async def db_session():
    """Create a test database with session."""
    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = Path(tmpdir) / "test.db"
        engine = await create_db_engine(db_path)
        await create_db_tables(engine)

        session_factory = async_sessionmaker(engine, class_=AsyncSession, expire_on_commit=False)
        async with session_factory() as session:
            yield session

        await engine.dispose()


@pytest.fixture
async def project(db_session: AsyncSession) -> Project:
    """Create a test project."""
    project = Project(name="Test Project")
    db_session.add(project)
    await db_session.commit()
    await db_session.refresh(project)
    return project


@pytest.fixture
async def repo(db_session: AsyncSession, project: Project) -> Repo:
    """Create a test repo."""
    repo = Repo(
        path="/code/app",
        name="app",
    )
    db_session.add(repo)
    await db_session.commit()
    await db_session.refresh(repo)
    return repo


class TestRepoModel:
    """Tests for Repo model with new fields."""

    @pytest.mark.asyncio
    async def test_repo_creation_with_new_fields(self, db_session: AsyncSession, project: Project):
        """Repo can be created with display_name and default_working_dir."""
        repo = Repo(
            path="/code/my-app",
            name="my-app",
            display_name="My Application",
            default_working_dir="/code/my-app/src",
            default_branch="main",
        )
        db_session.add(repo)
        await db_session.commit()
        await db_session.refresh(repo)

        assert repo.id is not None
        assert repo.display_name == "My Application"
        assert repo.default_working_dir == "/code/my-app/src"
        assert repo.default_branch == "main"

    @pytest.mark.asyncio
    async def test_repo_path_unique(self, db_session: AsyncSession, project: Project):
        """Repo path should be unique."""
        # Create first repo
        repo1 = Repo(
            path="/code/my-app",
            name="my-app",
        )
        db_session.add(repo1)
        await db_session.commit()

        # Try to create repo with same path - should fail with IntegrityError
        repo2 = Repo(
            path="/code/my-app",
            name="my-app-duplicate",
        )
        db_session.add(repo2)
        with pytest.raises(IntegrityError):
            await db_session.commit()


class TestProjectRepoJunction:
    """Tests for ProjectRepo junction table."""

    @pytest.mark.asyncio
    async def test_project_repo_link_creation(
        self, db_session: AsyncSession, project: Project, repo: Repo
    ):
        """ProjectRepo links a project to a repo."""
        link = ProjectRepo(
            project_id=project.id,
            repo_id=repo.id,
            is_primary=True,
            display_order=0,
        )
        db_session.add(link)
        await db_session.commit()
        await db_session.refresh(link)

        assert link.id is not None
        assert link.is_primary is True
        assert link.display_order == 0

    @pytest.mark.asyncio
    async def test_project_multiple_repos(self, db_session: AsyncSession, project: Project):
        """A project can have multiple repos via junction table."""
        # Create multiple repos
        repos = []
        for i, name in enumerate(["frontend", "backend", "shared"]):
            repo = Repo(
                path=f"/code/{name}",
                name=name,
            )
            db_session.add(repo)
            await db_session.commit()
            await db_session.refresh(repo)
            repos.append(repo)

            # Link to project
            link = ProjectRepo(
                project_id=project.id,
                repo_id=repo.id,
                is_primary=(i == 0),
                display_order=i,
            )
            db_session.add(link)

        await db_session.commit()

        # Verify links
        result = await db_session.execute(
            select(ProjectRepo).where(ProjectRepo.project_id == project.id)
        )
        links = result.scalars().all()
        assert len(links) == 3

    @pytest.mark.asyncio
    async def test_project_repo_unique_constraint(
        self, db_session: AsyncSession, project: Project, repo: Repo
    ):
        """Cannot create duplicate project-repo link."""
        link1 = ProjectRepo(
            project_id=project.id,
            repo_id=repo.id,
            is_primary=True,
        )
        db_session.add(link1)
        await db_session.commit()

        # Try to create duplicate link
        link2 = ProjectRepo(
            project_id=project.id,
            repo_id=repo.id,
            is_primary=False,
        )
        db_session.add(link2)
        with pytest.raises(IntegrityError):
            await db_session.commit()


class TestWorkspaceRepoJunction:
    """Tests for WorkspaceRepo junction table."""

    @pytest.mark.asyncio
    async def test_workspace_repo_link_creation(
        self, db_session: AsyncSession, project: Project, repo: Repo
    ):
        """WorkspaceRepo links a workspace to a repo with target branch."""
        workspace = Workspace(
            project_id=project.id,
            branch_name="kagan/abc123",
            path="/tmp/worktrees/abc123",
            status=WorkspaceStatus.ACTIVE,
        )
        db_session.add(workspace)
        await db_session.commit()
        await db_session.refresh(workspace)

        # Create workspace-repo link
        wr = WorkspaceRepo(
            workspace_id=workspace.id,
            repo_id=repo.id,
            target_branch="main",
            worktree_path="/tmp/worktrees/abc123/app",
        )
        db_session.add(wr)
        await db_session.commit()
        await db_session.refresh(wr)

        assert wr.id is not None
        assert wr.target_branch == "main"
        assert wr.worktree_path == "/tmp/worktrees/abc123/app"

    @pytest.mark.asyncio
    async def test_workspace_multiple_repos(self, db_session: AsyncSession, project: Project):
        """A workspace can have multiple repos via junction table."""
        # Create repos
        repos = []
        for name in ["frontend", "backend"]:
            repo = Repo(
                path=f"/code/{name}",
                name=name,
            )
            db_session.add(repo)
            await db_session.commit()
            await db_session.refresh(repo)
            repos.append(repo)

        # Create workspace (repo linkage is via WorkspaceRepo)
        workspace = Workspace(
            project_id=project.id,
            branch_name="kagan/feature",
            path="/tmp/worktrees/feature",
            status=WorkspaceStatus.ACTIVE,
        )
        db_session.add(workspace)
        await db_session.commit()
        await db_session.refresh(workspace)

        # Link workspace to both repos
        for repo in repos:
            wr = WorkspaceRepo(
                workspace_id=workspace.id,
                repo_id=repo.id,
                target_branch="main",
            )
            db_session.add(wr)

        await db_session.commit()

        # Verify links
        result = await db_session.execute(
            select(WorkspaceRepo).where(WorkspaceRepo.workspace_id == workspace.id)
        )
        links = result.scalars().all()
        assert len(links) == 2

    @pytest.mark.asyncio
    async def test_workspace_repo_unique_constraint(
        self, db_session: AsyncSession, project: Project, repo: Repo
    ):
        """Cannot create duplicate workspace-repo link."""
        workspace = Workspace(
            project_id=project.id,
            branch_name="kagan/test",
            path="/tmp/worktrees/test",
        )
        db_session.add(workspace)
        await db_session.commit()
        await db_session.refresh(workspace)

        wr1 = WorkspaceRepo(
            workspace_id=workspace.id,
            repo_id=repo.id,
            target_branch="main",
        )
        db_session.add(wr1)
        await db_session.commit()

        # Try to create duplicate
        wr2 = WorkspaceRepo(
            workspace_id=workspace.id,
            repo_id=repo.id,
            target_branch="develop",
        )
        db_session.add(wr2)
        with pytest.raises(IntegrityError):
            await db_session.commit()


class TestProjectLastOpenedAt:
    """Tests for Project.last_opened_at field."""

    @pytest.mark.asyncio
    async def test_project_last_opened_at_default_none(self, db_session: AsyncSession):
        """Project.last_opened_at defaults to None."""
        project = Project(name="New Project")
        db_session.add(project)
        await db_session.commit()
        await db_session.refresh(project)

        assert project.last_opened_at is None

    @pytest.mark.asyncio
    async def test_project_last_opened_at_can_be_set(self, db_session: AsyncSession):
        """Project.last_opened_at can be set."""
        now = datetime.now()
        project = Project(name="Opened Project", last_opened_at=now)
        db_session.add(project)
        await db_session.commit()
        await db_session.refresh(project)

        assert project.last_opened_at is not None

    @pytest.mark.asyncio
    async def test_project_last_opened_at_can_be_updated(self, db_session: AsyncSession):
        """Project.last_opened_at can be updated."""
        project = Project(name="Project")
        db_session.add(project)
        await db_session.commit()
        await db_session.refresh(project)

        assert project.last_opened_at is None

        # Update
        now = datetime.now()
        project.last_opened_at = now
        db_session.add(project)
        await db_session.commit()
        await db_session.refresh(project)

        assert project.last_opened_at is not None


class TestMergeRepoId:
    """Tests for Merge.repo_id field."""

    @pytest.mark.asyncio
    async def test_merge_repo_id_default_none(
        self, db_session: AsyncSession, project: Project, repo: Repo
    ):
        """Merge.repo_id defaults to None."""
        task = Task(
            project_id=project.id,
            title="Test Task",
            status=TaskStatus.IN_PROGRESS,
        )
        db_session.add(task)
        await db_session.commit()
        await db_session.refresh(task)

        # Create merge without repo_id
        merge = Merge(task_id=task.id)
        db_session.add(merge)
        await db_session.commit()
        await db_session.refresh(merge)

        assert merge.repo_id is None

    @pytest.mark.asyncio
    async def test_merge_repo_id_can_be_set(
        self, db_session: AsyncSession, project: Project, repo: Repo
    ):
        """Merge.repo_id can be set to a repo."""
        task = Task(
            project_id=project.id,
            title="Test Task",
            status=TaskStatus.IN_PROGRESS,
        )
        db_session.add(task)
        await db_session.commit()
        await db_session.refresh(task)

        # Create merge with repo_id
        merge = Merge(task_id=task.id, repo_id=repo.id)
        db_session.add(merge)
        await db_session.commit()
        await db_session.refresh(merge)

        assert merge.repo_id == repo.id


class TestQueryPatterns:
    """Tests for common query patterns with junction tables."""

    @pytest.mark.asyncio
    async def test_query_repos_for_project(self, db_session: AsyncSession, project: Project):
        """Query all repos for a project via junction table."""
        # Create repos and links
        for i, name in enumerate(["frontend", "backend"]):
            repo = Repo(
                path=f"/code/{name}",
                name=name,
            )
            db_session.add(repo)
            await db_session.commit()
            await db_session.refresh(repo)

            link = ProjectRepo(
                project_id=project.id,
                repo_id=repo.id,
                display_order=i,
            )
            db_session.add(link)

        await db_session.commit()

        # Query repos via junction
        result = await db_session.execute(
            select(ProjectRepo)
            .where(ProjectRepo.project_id == project.id)
            .order_by(col(ProjectRepo.display_order))
        )
        links = result.scalars().all()

        assert len(links) == 2
        # Get repo IDs in order
        repo_ids = [link.repo_id for link in links]
        assert len(repo_ids) == 2

    @pytest.mark.asyncio
    async def test_query_repos_for_workspace(self, db_session: AsyncSession, project: Project):
        """Query all repos for a workspace via junction table."""
        # Create repos
        repos = []
        for name in ["frontend", "backend"]:
            repo = Repo(
                path=f"/code/{name}",
                name=name,
            )
            db_session.add(repo)
            await db_session.commit()
            await db_session.refresh(repo)
            repos.append(repo)

        # Create workspace
        workspace = Workspace(
            project_id=project.id,
            branch_name="kagan/test",
            path="/tmp/test",
        )
        db_session.add(workspace)
        await db_session.commit()
        await db_session.refresh(workspace)

        # Create workspace-repo links
        for repo in repos:
            link = WorkspaceRepo(
                workspace_id=workspace.id,
                repo_id=repo.id,
                target_branch="main",
            )
            db_session.add(link)

        await db_session.commit()

        # Query repos via junction
        result = await db_session.execute(
            select(WorkspaceRepo).where(WorkspaceRepo.workspace_id == workspace.id)
        )
        links = result.scalars().all()

        assert len(links) == 2
        target_branches = {link.target_branch for link in links}
        assert target_branches == {"main"}
