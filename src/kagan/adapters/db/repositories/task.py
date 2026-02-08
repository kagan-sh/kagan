"""Primary task repository and shared aggregate mixins."""

from __future__ import annotations

import asyncio
from pathlib import Path
from typing import TYPE_CHECKING, Any

from sqlalchemy import case, func, or_
from sqlalchemy.ext.asyncio import AsyncEngine, AsyncSession, async_sessionmaker
from sqlmodel import col, delete, select

from kagan.adapters.db.engine import create_db_engine, create_db_tables
from kagan.adapters.db.repositories.base import ClosingAwareSessionFactory
from kagan.adapters.db.schema import Project, Task, TaskLink, TaskStatus
from kagan.core.time import utc_now
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
        self._session_factory: ClosingAwareSessionFactory | None = None
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
        raw_factory = async_sessionmaker(self._engine, class_=AsyncSession, expire_on_commit=False)
        self._session_factory = ClosingAwareSessionFactory(raw_factory)
        await create_db_tables(self._engine)
        await self._ensure_schema_compatibility()
        await self._ensure_defaults()

    async def close(self) -> None:
        """Close engine and release resources."""
        if self._session_factory is not None:
            self._session_factory.mark_closing()
        if self._engine:
            await self._engine.dispose()
            self._engine = None
            self._session_factory = None

    def mark_closing(self) -> None:
        """Signal that shutdown has started."""
        if self._session_factory is not None:
            self._session_factory.mark_closing()

    def _get_session(self) -> AsyncSession:
        """Get a new async session."""
        assert self._session_factory, "Repository not initialized"
        return self._session_factory()

    @property
    def session_factory(self) -> ClosingAwareSessionFactory:
        """Public session factory accessor for downstream service wiring."""
        assert self._session_factory is not None, "Repository not initialized"
        return self._session_factory

    async def _ensure_schema_compatibility(self) -> None:
        """Apply lightweight compatibility shims for legacy databases."""
        assert self._engine is not None, "Repository not initialized"
        async with self._engine.begin() as conn:
            result = await conn.exec_driver_sql("PRAGMA table_info(tasks)")
            column_names = {str(row[1]) for row in result.fetchall()}
            if "terminal_backend" in column_names:
                return
            await conn.exec_driver_sql("ALTER TABLE tasks ADD COLUMN terminal_backend VARCHAR")

    async def _ensure_defaults(self) -> None:
        """Initialize database - no auto-creation of projects or repos."""
        pass

    async def ensure_test_project(self, name: str = "Test Project") -> str:
        """Create a test project and return its ID."""
        async with self._get_session() as session:
            result = await session.execute(select(Project).order_by(col(Project.created_at).asc()))
            project = result.scalars().first()
            if project is None:
                project = Project(name=name, description="")
                session.add(project)
                await session.commit()
                await session.refresh(project)

            self._default_project_id = project.id
            return project.id

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

    async def get_all(self, *, project_id: str | None = None) -> Sequence[Task]:
        """Get all tasks ordered by status, priority, created_at."""
        async with self._get_session() as session:
            query = select(Task)
            if project_id is not None:
                query = query.where(Task.project_id == project_id)
            result = await session.execute(
                query.order_by(
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

    async def get_by_status(
        self, status: TaskStatus, *, project_id: str | None = None
    ) -> Sequence[Task]:
        """Get all tasks with a specific status."""
        async with self._get_session() as session:
            query = select(Task).where(Task.status == status)
            if project_id is not None:
                query = query.where(Task.project_id == project_id)
            result = await session.execute(
                query.order_by(col(Task.priority).desc(), col(Task.created_at).asc())
            )
            return result.scalars().all()

    async def get_tasks_by_ids(
        self, task_ids: set[str], *, project_id: str | None = None
    ) -> Sequence[Task]:
        """Get tasks matching a set of IDs."""
        if not task_ids:
            return []

        async with self._get_session() as session:
            query = select(Task).where(col(Task.id).in_(task_ids))
            if project_id is not None:
                query = query.where(Task.project_id == project_id)
            result = await session.execute(query)
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
                task.updated_at = utc_now()

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
                await session.execute(
                    delete(TaskLink).where(
                        or_(
                            col(TaskLink.task_id) == task_id,
                            col(TaskLink.ref_task_id) == task_id,
                        )
                    )
                )
                await session.delete(task)
                await session.commit()

        self._notify_change(task_id)
        self._notify_status_change(task_id, old_status, None)
        return True

    async def move(self, task_id: str, new_status: TaskStatus) -> Task | None:
        """Move a task to a new status."""
        return await self.update(task_id, status=new_status)

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
                    (col(Task.id) == query)
                    | (col(Task.title).ilike(pattern))
                    | (col(Task.description).ilike(pattern))
                )
                .order_by(col(Task.updated_at).desc())
            )
            return result.scalars().all()

    async def replace_task_links(self, task_id: str, ref_task_ids: set[str]) -> None:
        """Replace all references for a task."""
        async with self._lock:
            async with self._get_session() as session:
                await session.execute(delete(TaskLink).where(col(TaskLink.task_id) == task_id))
                for ref_id in sorted(ref_task_ids):
                    if ref_id == task_id:
                        continue
                    session.add(TaskLink(task_id=task_id, ref_task_id=ref_id))
                await session.commit()

    async def get_task_links(self, task_id: str) -> list[str]:
        """Return referenced task IDs for a task."""
        async with self._get_session() as session:
            result = await session.execute(
                select(TaskLink.ref_task_id).where(TaskLink.task_id == task_id)
            )
            return list(result.scalars().all())

    @property
    def default_project_id(self) -> str | None:
        """Return default project ID (if initialized)."""
        return self._default_project_id

    def set_default_project_id(self, project_id: str | None) -> None:
        """Set default project ID for task creation."""
        self._default_project_id = project_id

    @property
    def default_branch(self) -> str:
        """Return default branch name."""
        return self._default_branch
