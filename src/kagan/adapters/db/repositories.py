"""Async repositories for domain entities."""

from __future__ import annotations

import asyncio
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, Any

from sqlalchemy import case, func
from sqlalchemy.ext.asyncio import AsyncEngine, AsyncSession, async_sessionmaker
from sqlmodel import col, select

from kagan.adapters.db.engine import create_db_engine, create_db_tables
from kagan.adapters.db.schema import (
    AgentTurn,
    AgentTurnKind,
    ExecutionProcess,
    MergeReadiness,
    Project,
    ProjectRepo,
    Repo,
    Scratch,
    Task,
    TaskStatus,
)
from kagan.limits import SCRATCHPAD_LIMIT
from kagan.paths import get_database_path

if TYPE_CHECKING:
    from collections.abc import Callable, Sequence


class TaskRepository:
    """Async repository for task operations."""

    def __init__(
        self,
        db_path: str | Path | None = None,
        *,
        project_root: Path | None = None,
        default_branch: str = "main",
        on_change: Callable[[str], None] | None = None,
    ) -> None:
        self.db_path = Path(db_path) if db_path else get_database_path()
        self._engine: AsyncEngine | None = None
        self._session_factory: async_sessionmaker[AsyncSession] | None = None
        self._lock = asyncio.Lock()
        self._on_change = on_change
        self._on_status_change: (
            Callable[[str, TaskStatus | None, TaskStatus | None], None] | None
        ) = None
        self._project_root = project_root or Path.cwd()
        self._default_branch = default_branch
        self._default_project_id: str | None = None

    async def initialize(self) -> None:
        """Initialize engine and create tables."""
        self._engine = await create_db_engine(self.db_path)
        self._session_factory = async_sessionmaker(
            self._engine, class_=AsyncSession, expire_on_commit=False
        )
        await create_db_tables(self._engine)
        await self._ensure_defaults()

    async def close(self) -> None:
        """Close engine and release resources."""
        if self._engine:
            await self._engine.dispose()
            self._engine = None
            self._session_factory = None

    def _get_session(self) -> AsyncSession:
        """Get a new async session."""
        assert self._session_factory, "Repository not initialized"
        return self._session_factory()

    async def _ensure_defaults(self) -> None:
        """Ensure a default project and repo exist."""
        from kagan.git_utils import has_git_repo

        async with self._get_session() as session:
            result = await session.execute(select(Project).order_by(col(Project.created_at).asc()))
            project = result.scalars().first()
            if project is None:
                project = Project(
                    name="Default Project",
                    description="",
                )
                session.add(project)
                await session.commit()
                await session.refresh(project)

            if not await has_git_repo(self._project_root):
                self._default_project_id = project.id
                return

            repo = None
            resolved_root = str(self._project_root.resolve())
            result = await session.execute(select(Repo).where(Repo.path == resolved_root))
            repo = result.scalars().first()
            if repo is None:
                repo = Repo(
                    name=self._project_root.name or "repo",
                    path=resolved_root,
                    default_branch=self._default_branch,
                )
                session.add(repo)
                await session.commit()
                await session.refresh(repo)

            if project.id and repo.id:
                link_result = await session.execute(
                    select(ProjectRepo).where(
                        ProjectRepo.project_id == project.id,
                        ProjectRepo.repo_id == repo.id,
                    )
                )
                if link_result.scalars().first() is None:
                    link = ProjectRepo(
                        project_id=project.id,
                        repo_id=repo.id,
                        is_primary=True,
                        display_order=0,
                    )
                    session.add(link)
                    await session.commit()

            self._default_project_id = project.id

    def set_status_change_callback(
        self,
        callback: Callable[[str, TaskStatus | None, TaskStatus | None], None] | None,
    ) -> None:
        """Set callback for task status changes."""
        self._on_status_change = callback

    def _notify_change(self, task_id: str) -> None:
        if self._on_change:
            self._on_change(task_id)

    def _notify_status_change(
        self,
        task_id: str,
        old_status: TaskStatus | None,
        new_status: TaskStatus | None,
    ) -> None:
        if self._on_status_change:
            self._on_status_change(task_id, old_status, new_status)

    async def create(self, task: Task) -> Task:
        """Create a new task."""
        async with self._lock:
            async with self._get_session() as session:
                session.add(task)
                await session.commit()
                await session.refresh(task)

        if task.id:
            self._notify_change(task.id)
            self._notify_status_change(task.id, None, task.status)
        return task

    async def get(self, task_id: str) -> Task | None:
        """Get a task by ID."""
        async with self._get_session() as session:
            return await session.get(Task, task_id)

    async def get_all(self) -> Sequence[Task]:
        """Get all tasks ordered by status, priority, created_at."""
        async with self._get_session() as session:
            result = await session.execute(
                select(Task).order_by(
                    case(
                        (col(Task.status) == TaskStatus.BACKLOG, 0),
                        (col(Task.status) == TaskStatus.IN_PROGRESS, 1),
                        (col(Task.status) == TaskStatus.REVIEW, 2),
                        (col(Task.status) == TaskStatus.DONE, 3),
                        else_=99,
                    ),
                    col(Task.priority).desc(),
                    col(Task.created_at).asc(),
                )
            )
            return result.scalars().all()

    async def get_by_status(self, status: TaskStatus) -> Sequence[Task]:
        """Get all tasks with a specific status."""
        async with self._get_session() as session:
            result = await session.execute(
                select(Task)
                .where(Task.status == status)
                .order_by(col(Task.priority).desc(), col(Task.created_at).asc())
            )
            return result.scalars().all()

    async def update(self, task_id: str, **kwargs: Any) -> Task | None:
        """Update a task with keyword arguments."""
        async with self._lock:
            async with self._get_session() as session:
                task = await session.get(Task, task_id)
                if not task:
                    return None

                old_status = task.status
                update_data = {k: v for k, v in kwargs.items() if v is not None}
                if update_data:
                    task.sqlmodel_update(update_data)
                task.updated_at = datetime.now()

                session.add(task)
                await session.commit()
                await session.refresh(task)

                if "status" in update_data and update_data["status"] != old_status:
                    self._notify_status_change(task_id, old_status, update_data["status"])

                self._notify_change(task_id)
                return task

    async def delete(self, task_id: str) -> bool:
        """Delete a task. Returns True if deleted."""
        async with self._lock:
            async with self._get_session() as session:
                task = await session.get(Task, task_id)
                if not task:
                    return False

                old_status = task.status
                await session.delete(task)
                await session.commit()

        self._notify_change(task_id)
        self._notify_status_change(task_id, old_status, None)
        return True

    async def move(self, task_id: str, new_status: TaskStatus) -> Task | None:
        """Move a task to a new status."""
        return await self.update(task_id, status=new_status)

    async def mark_session_active(self, task_id: str, active: bool) -> Task | None:
        """Mark task session as active/inactive."""
        return await self.update(task_id, session_active=active)

    async def set_review_summary(
        self, task_id: str, summary: str, checks_passed: bool | None
    ) -> Task | None:
        """Set review summary and checks status."""
        return await self.update(task_id, review_summary=summary, checks_passed=checks_passed)

    async def increment_total_iterations(self, task_id: str) -> None:
        """Increment the total_iterations counter."""
        async with self._lock:
            async with self._get_session() as session:
                task = await session.get(Task, task_id)
                if task:
                    task.total_iterations += 1
                    session.add(task)
                    await session.commit()

    async def get_counts(self) -> dict[TaskStatus, int]:
        """Get task counts by status."""
        async with self._get_session() as session:
            result = await session.execute(
                select(Task.status, func.count(col(Task.id))).group_by(Task.status)
            )
            counts = {status: 0 for status in TaskStatus}
            for status, count in result.all():
                counts[status] = count
            return counts

    async def search(self, query: str) -> Sequence[Task]:
        """Search tasks by title, description, or ID."""
        if not query or not query.strip():
            return []

        query = query.strip()
        pattern = f"%{query}%"

        async with self._get_session() as session:
            result = await session.execute(
                select(Task)
                .where(
                    (Task.id == query)
                    | (col(Task.title).ilike(pattern))
                    | (col(Task.description).ilike(pattern))
                )
                .order_by(col(Task.updated_at).desc())
            )
            return result.scalars().all()

    async def get_scratchpad(self, task_id: str) -> str:
        """Get scratchpad content for a task."""
        async with self._get_session() as session:
            scratchpad = await session.get(Scratch, task_id)
            return scratchpad.content if scratchpad else ""

    async def update_scratchpad(self, task_id: str, content: str) -> None:
        """Update or create scratchpad content."""
        content = content[-SCRATCHPAD_LIMIT:] if len(content) > SCRATCHPAD_LIMIT else content

        async with self._lock:
            async with self._get_session() as session:
                scratchpad = await session.get(Scratch, task_id)
                if scratchpad:
                    scratchpad.content = content
                    scratchpad.updated_at = datetime.now()
                else:
                    scratchpad = Scratch(task_id=task_id, content=content)
                session.add(scratchpad)
                await session.commit()

    async def delete_scratchpad(self, task_id: str) -> None:
        """Delete scratchpad for a task."""
        async with self._lock:
            async with self._get_session() as session:
                scratchpad = await session.get(Scratch, task_id)
                if scratchpad:
                    await session.delete(scratchpad)
                    await session.commit()

    async def _ensure_execution(self, session: AsyncSession, task_id: str) -> ExecutionProcess:
        execution = await session.get(ExecutionProcess, task_id)
        if execution is None:
            execution = ExecutionProcess(
                id=task_id,
                task_id=task_id,
                executor="automation",
            )
            session.add(execution)
            await session.commit()
            await session.refresh(execution)
        return execution

    async def append_agent_log(
        self, task_id: str, log_type: str, iteration: int, content: str
    ) -> None:
        """Append a log entry for agent execution."""
        async with self._lock:
            async with self._get_session() as session:
                execution = await self._ensure_execution(session, task_id)
                log = AgentTurn(
                    execution_id=execution.id,
                    kind=AgentTurnKind.LOG,
                    source=log_type,
                    sequence=iteration,
                    content=content,
                )
                session.add(log)
                await session.commit()

    async def get_agent_logs(self, task_id: str, log_type: str) -> Sequence[AgentTurn]:
        """Get all log entries for a task and log type."""
        async with self._get_session() as session:
            execution = await self._ensure_execution(session, task_id)
            result = await session.execute(
                select(AgentTurn)
                .where(
                    AgentTurn.execution_id == execution.id,
                    AgentTurn.kind == AgentTurnKind.LOG,
                    AgentTurn.source == log_type,
                )
                .order_by(col(AgentTurn.sequence).asc(), col(AgentTurn.created_at).asc())
            )
            return result.scalars().all()

    async def clear_agent_logs(self, task_id: str) -> None:
        """Clear all agent logs for a task."""
        async with self._lock:
            async with self._get_session() as session:
                execution = await self._ensure_execution(session, task_id)
                result = await session.execute(
                    select(AgentTurn).where(
                        AgentTurn.execution_id == execution.id,
                        AgentTurn.kind == AgentTurnKind.LOG,
                    )
                )
                for log in result.scalars().all():
                    await session.delete(log)
                await session.commit()

    async def append_event(self, task_id: str, event_type: str, message: str) -> None:
        """Append an audit event for a task."""
        async with self._lock:
            async with self._get_session() as session:
                execution = await self._ensure_execution(session, task_id)
                event = AgentTurn(
                    execution_id=execution.id,
                    kind=AgentTurnKind.EVENT,
                    source=event_type,
                    content=message,
                )
                session.add(event)
                await session.commit()

    async def get_events(self, task_id: str, limit: int = 20) -> Sequence[AgentTurn]:
        """Get recent audit events for a task."""
        async with self._get_session() as session:
            execution = await self._ensure_execution(session, task_id)
            result = await session.execute(
                select(AgentTurn)
                .where(
                    AgentTurn.execution_id == execution.id,
                    AgentTurn.kind == AgentTurnKind.EVENT,
                )
                .order_by(col(AgentTurn.created_at).desc(), col(AgentTurn.id).desc())
                .limit(limit)
            )
            return result.scalars().all()

    async def sync_status_from_agent_complete(self, task_id: str, success: bool) -> Task | None:
        """Auto-transition task when agent completes."""
        task = await self.get(task_id)
        if not task:
            return None

        if success and task.status == TaskStatus.IN_PROGRESS:
            return await self.update(
                task_id,
                status=TaskStatus.REVIEW,
                session_active=False,
                last_error=None,
            )
        if not success:
            return await self.update(task_id, session_active=False)
        return task

    async def sync_status_from_review_pass(self, task_id: str) -> Task | None:
        """Auto-transition task when review passes (REVIEW -> DONE)."""
        task = await self.get(task_id)
        if not task or task.status != TaskStatus.REVIEW:
            return task

        return await self.update(
            task_id,
            status=TaskStatus.DONE,
            checks_passed=True,
            merge_readiness=MergeReadiness.READY,
        )

    async def sync_status_from_review_reject(
        self, task_id: str, reason: str | None = None
    ) -> Task | None:
        """Move task back to IN_PROGRESS after review rejection."""
        task = await self.get(task_id)
        if not task or task.status != TaskStatus.REVIEW:
            return task

        return await self.update(
            task_id,
            status=TaskStatus.IN_PROGRESS,
            checks_passed=False,
            review_summary=reason,
        )

    @property
    def default_project_id(self) -> str | None:
        """Return default project ID (if initialized)."""
        return self._default_project_id

    @property
    def default_branch(self) -> str:
        """Return default branch name."""
        return self._default_branch


class RepoRepository:
    """CRUD operations for Repo entities."""

    def __init__(self, session_factory: async_sessionmaker[AsyncSession]) -> None:
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
        from kagan.adapters.db.schema import ProjectRepo

        async with self._get_session() as session:
            result = await session.execute(
                select(ProjectRepo)
                .where(ProjectRepo.project_id == project_id)
                .order_by(col(ProjectRepo.display_order))
            )
            links = result.scalars().all()
            repos = []
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
        from kagan.adapters.db.schema import ProjectRepo

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
        from kagan.adapters.db.schema import ProjectRepo

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
