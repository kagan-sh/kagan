"""Async repositories for domain entities."""

from __future__ import annotations

import asyncio
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, Any

from sqlalchemy import case, func, or_
from sqlalchemy.ext.asyncio import AsyncEngine, AsyncSession, async_sessionmaker
from sqlmodel import col, delete, select

from kagan.adapters.db.engine import create_db_engine, create_db_tables
from kagan.adapters.db.schema import (
    CodingAgentTurn,
    ExecutionProcess,
    ExecutionProcessLog,
    ExecutionProcessRepoState,
    Project,
    ProjectRepo,
    Repo,
    Scratch,
    Session,
    Task,
    TaskLink,
    TaskStatus,
    Workspace,
)
from kagan.core.models.enums import (
    ExecutionRunReason,
    ExecutionStatus,
    ScratchType,
    SessionStatus,
    SessionType,
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
        await self._ensure_schema_compatibility()
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
        """Initialize database - no auto-creation of projects or repos.

        Projects and repos are only created through explicit user action via
        WelcomeScreen. Auto-creating projects caused a bug where first-time
        users bypassed WelcomeScreen entirely.

        For testing, use TaskRepository.ensure_test_project() to create a
        test project explicitly.
        """

        pass

    async def ensure_test_project(self, name: str = "Test Project") -> str:
        """Create a test project and return its ID. For testing only.

        This method provides an explicit way for tests to create a project
        without relying on production auto-creation behavior.

        Args:
            name: Name for the test project

        Returns:
            The project ID
        """
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

    async def create_session_record(
        self,
        *,
        workspace_id: str,
        session_type: SessionType,
        external_id: str | None = None,
    ) -> Session:
        """Create a session record."""
        async with self._lock:
            async with self._get_session() as session:
                record = Session(
                    workspace_id=workspace_id,
                    session_type=session_type,
                    status=SessionStatus.ACTIVE,
                    external_id=external_id,
                    started_at=datetime.now(),
                    ended_at=None,
                )
                session.add(record)
                await session.commit()
                await session.refresh(record)
                return record

    async def close_session_record(
        self,
        session_id: str,
        *,
        status: SessionStatus = SessionStatus.CLOSED,
    ) -> Session | None:
        """Close a session record."""
        async with self._lock:
            async with self._get_session() as session:
                record = await session.get(Session, session_id)
                if record is None:
                    return None
                record.status = status
                record.ended_at = datetime.now()
                session.add(record)
                await session.commit()
                await session.refresh(record)
                return record

    async def close_session_by_external_id(
        self,
        external_id: str,
        *,
        status: SessionStatus = SessionStatus.CLOSED,
    ) -> Session | None:
        """Close a session record by external ID."""
        async with self._lock:
            async with self._get_session() as session:
                result = await session.execute(
                    select(Session).where(Session.external_id == external_id)
                )
                record = result.scalars().first()
                if record is None:
                    return None
                record.status = status
                record.ended_at = datetime.now()
                session.add(record)
                await session.commit()
                await session.refresh(record)
                return record

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

    async def get_scratchpad(self, task_id: str) -> str:
        """Get scratchpad content for a task."""
        async with self._get_session() as session:
            result = await session.execute(
                select(Scratch).where(
                    Scratch.id == task_id,
                    Scratch.scratch_type == ScratchType.WORKSPACE_NOTES,
                )
            )
            scratchpad = result.scalars().first()
            if not scratchpad:
                return ""
            payload = scratchpad.payload or {}
            return str(payload.get("content", ""))

    async def update_scratchpad(self, task_id: str, content: str) -> None:
        """Update or create scratchpad content."""
        content = content[-SCRATCHPAD_LIMIT:] if len(content) > SCRATCHPAD_LIMIT else content

        async with self._lock:
            async with self._get_session() as session:
                result = await session.execute(
                    select(Scratch).where(
                        Scratch.id == task_id,
                        Scratch.scratch_type == ScratchType.WORKSPACE_NOTES,
                    )
                )
                scratchpad = result.scalars().first()
                if scratchpad:
                    scratchpad.payload = {"content": content}
                    scratchpad.updated_at = datetime.now()
                else:
                    scratchpad = Scratch(
                        id=task_id,
                        scratch_type=ScratchType.WORKSPACE_NOTES,
                        payload={"content": content},
                    )
                    scratchpad.created_at = datetime.now()
                    scratchpad.updated_at = datetime.now()
                session.add(scratchpad)
                await session.commit()

    async def delete_scratchpad(self, task_id: str) -> None:
        """Delete scratchpad for a task."""
        async with self._lock:
            async with self._get_session() as session:
                result = await session.execute(
                    select(Scratch).where(
                        Scratch.id == task_id,
                        Scratch.scratch_type == ScratchType.WORKSPACE_NOTES,
                    )
                )
                scratchpad = result.scalars().first()
                if scratchpad:
                    await session.delete(scratchpad)
                    await session.commit()

    async def create_execution(
        self,
        *,
        session_id: str,
        run_reason: ExecutionRunReason,
        executor_action: dict[str, Any] | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> ExecutionProcess:
        """Create a new execution process."""
        async with self._lock:
            async with self._get_session() as session:
                execution = ExecutionProcess(
                    session_id=session_id,
                    run_reason=run_reason,
                    executor_action=executor_action or {},
                    status=ExecutionStatus.RUNNING,
                    metadata_=metadata or {},
                    started_at=datetime.now(),
                    created_at=datetime.now(),
                    updated_at=datetime.now(),
                )
                session.add(execution)
                await session.commit()
                await session.refresh(execution)
                return execution

    async def update_execution(self, execution_id: str, **kwargs: Any) -> ExecutionProcess | None:
        """Update an execution process."""
        async with self._lock:
            async with self._get_session() as session:
                execution = await session.get(ExecutionProcess, execution_id)
                if not execution:
                    return None
                update_data = {k: v for k, v in kwargs.items() if v is not None}
                if "metadata" in update_data and "metadata_" not in update_data:
                    update_data["metadata_"] = update_data.pop("metadata")
                if update_data:
                    execution.sqlmodel_update(update_data)
                execution.updated_at = datetime.now()
                session.add(execution)
                await session.commit()
                await session.refresh(execution)
                return execution

    async def append_execution_log(self, execution_id: str, log_line: str) -> ExecutionProcessLog:
        """Append a JSONL log line for an execution."""
        async with self._lock:
            async with self._get_session() as session:
                log_entry = ExecutionProcessLog(
                    execution_process_id=execution_id,
                    logs=log_line,
                    byte_size=len(log_line.encode("utf-8")),
                    inserted_at=datetime.now(),
                )
                session.add(log_entry)
                await session.commit()
                await session.refresh(log_entry)
                return log_entry

    async def get_execution_logs(self, execution_id: str) -> ExecutionProcessLog | None:
        """Return aggregated execution logs for an execution."""
        async with self._get_session() as session:
            result = await session.execute(
                select(ExecutionProcessLog)
                .where(ExecutionProcessLog.execution_process_id == execution_id)
                .order_by(
                    col(ExecutionProcessLog.inserted_at).asc(),
                    col(ExecutionProcessLog.id).asc(),
                )
            )
            entries = result.scalars().all()
            if not entries:
                return None
            combined_logs = "\n".join(entry.logs for entry in entries if entry.logs)
            total_bytes = sum(entry.byte_size for entry in entries)
            latest = entries[-1]
            return ExecutionProcessLog(
                id=latest.id,
                execution_process_id=execution_id,
                logs=combined_logs,
                byte_size=total_bytes,
                inserted_at=latest.inserted_at,
            )

    async def get_execution_log_entries(self, execution_id: str) -> list[ExecutionProcessLog]:
        async with self._get_session() as session:
            result = await session.execute(
                select(ExecutionProcessLog)
                .where(ExecutionProcessLog.execution_process_id == execution_id)
                .order_by(
                    col(ExecutionProcessLog.inserted_at).asc(),
                    col(ExecutionProcessLog.id).asc(),
                )
            )
            return list(result.scalars().all())

    async def get_execution(self, execution_id: str) -> ExecutionProcess | None:
        """Return execution record by ID."""
        async with self._get_session() as session:
            return await session.get(ExecutionProcess, execution_id)

    async def append_agent_turn(
        self,
        execution_id: str,
        *,
        agent_session_id: str | None = None,
        prompt: str | None = None,
        summary: str | None = None,
        agent_message_id: str | None = None,
    ) -> CodingAgentTurn:
        """Append a coding agent turn."""
        async with self._lock:
            async with self._get_session() as session:
                turn = CodingAgentTurn(
                    execution_process_id=execution_id,
                    agent_session_id=agent_session_id,
                    prompt=prompt,
                    summary=summary,
                    agent_message_id=agent_message_id,
                    seen=False,
                    created_at=datetime.now(),
                    updated_at=datetime.now(),
                )
                session.add(turn)
                await session.commit()
                await session.refresh(turn)
                return turn

    async def list_agent_turns(self, execution_id: str) -> Sequence[CodingAgentTurn]:
        """List coding agent turns for an execution."""
        async with self._get_session() as session:
            result = await session.execute(
                select(CodingAgentTurn)
                .where(CodingAgentTurn.execution_process_id == execution_id)
                .order_by(col(CodingAgentTurn.created_at).asc(), col(CodingAgentTurn.id).asc())
            )
            return result.scalars().all()

    async def get_latest_agent_turn_for_execution(
        self, execution_id: str
    ) -> CodingAgentTurn | None:
        """Return the latest coding agent turn for an execution."""
        async with self._get_session() as session:
            result = await session.execute(
                select(CodingAgentTurn)
                .where(CodingAgentTurn.execution_process_id == execution_id)
                .order_by(col(CodingAgentTurn.created_at).desc(), col(CodingAgentTurn.id).desc())
                .limit(1)
            )
            return result.scalars().first()

    async def add_execution_repo_state(
        self,
        execution_id: str,
        repo_id: str,
        *,
        before_head_commit: str | None = None,
        after_head_commit: str | None = None,
        merge_commit: str | None = None,
    ) -> ExecutionProcessRepoState:
        """Persist per-repo state for an execution."""
        async with self._lock:
            async with self._get_session() as session:
                state = ExecutionProcessRepoState(
                    execution_process_id=execution_id,
                    repo_id=repo_id,
                    before_head_commit=before_head_commit,
                    after_head_commit=after_head_commit,
                    merge_commit=merge_commit,
                    created_at=datetime.now(),
                    updated_at=datetime.now(),
                )
                session.add(state)
                await session.commit()
                await session.refresh(state)
                return state

    async def get_latest_execution_for_task(self, task_id: str) -> ExecutionProcess | None:
        """Return most recent execution for a task."""
        async with self._get_session() as session:
            result = await session.execute(
                select(ExecutionProcess)
                .join(Session, col(ExecutionProcess.session_id) == col(Session.id))
                .join(Workspace, col(Session.workspace_id) == col(Workspace.id))
                .where(Workspace.task_id == task_id)
                .order_by(col(ExecutionProcess.created_at).desc())
                .limit(1)
            )
            return result.scalars().first()

    async def get_latest_execution_for_session(self, session_id: str) -> ExecutionProcess | None:
        """Return most recent execution for a session."""
        async with self._get_session() as session:
            result = await session.execute(
                select(ExecutionProcess)
                .where(ExecutionProcess.session_id == session_id)
                .order_by(col(ExecutionProcess.created_at).desc())
                .limit(1)
            )
            return result.scalars().first()

    async def get_running_execution_for_session(self, session_id: str) -> ExecutionProcess | None:
        """Return running execution for a session, if any."""
        async with self._get_session() as session:
            result = await session.execute(
                select(ExecutionProcess)
                .where(
                    ExecutionProcess.session_id == session_id,
                    ExecutionProcess.status == ExecutionStatus.RUNNING,
                )
                .order_by(col(ExecutionProcess.created_at).desc())
                .limit(1)
            )
            return result.scalars().first()

    async def count_executions_for_task(self, task_id: str) -> int:
        """Return total executions for a task."""
        async with self._get_session() as session:
            result = await session.execute(
                select(func.count())
                .select_from(ExecutionProcess)
                .join(Session, col(ExecutionProcess.session_id) == col(Session.id))
                .join(Workspace, col(Session.workspace_id) == col(Workspace.id))
                .where(Workspace.task_id == task_id)
            )
            return int(result.scalar_one() or 0)

    async def sync_status_from_agent_complete(self, task_id: str, success: bool) -> Task | None:
        """Auto-transition task when agent completes."""
        task = await self.get(task_id)
        if not task:
            return None

        if success and task.status == TaskStatus.IN_PROGRESS:
            return await self.update(task_id, status=TaskStatus.REVIEW)
        return task

    async def sync_status_from_review_pass(self, task_id: str) -> Task | None:
        """Auto-transition task when review passes (REVIEW -> DONE)."""
        task = await self.get(task_id)
        if not task or task.status != TaskStatus.REVIEW:
            return task

        return await self.update(task_id, status=TaskStatus.DONE)

    async def sync_status_from_review_reject(
        self, task_id: str, reason: str | None = None
    ) -> Task | None:
        """Move task back to IN_PROGRESS after review rejection."""
        task = await self.get(task_id)
        if not task or task.status != TaskStatus.REVIEW:
            return task

        return await self.update(task_id, status=TaskStatus.IN_PROGRESS)

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
