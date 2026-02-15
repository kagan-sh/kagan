"""Auxiliary repositories for audit, scratchpads, session records, planners, and repos."""

from __future__ import annotations

import asyncio
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, Any

from sqlalchemy.exc import IntegrityError
from sqlmodel import col, select

from kagan.core.adapters.db.schema import (
    AuditEvent,
    PlannerProposal,
    ProjectRepo,
    Repo,
    Scratch,
    Session,
    WorkspaceRepo,
)
from kagan.core.constants import KAGAN_BRANCH_CONFIGURED_KEY
from kagan.core.limits import SCRATCHPAD_LIMIT
from kagan.core.models.enums import ProposalStatus, ScratchType, SessionStatus, SessionType
from kagan.core.time import utc_now

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession

    from kagan.core.adapters.db.repositories.base import ClosingAwareSessionFactory


class AuditRepository:
    """Immutable audit log repository."""

    def __init__(self, session_factory: ClosingAwareSessionFactory) -> None:
        self._session_factory = session_factory
        self._lock = asyncio.Lock()

    def _get_session(self) -> AsyncSession:
        return self._session_factory()

    async def record(
        self,
        *,
        actor_type: str,
        actor_id: str,
        session_id: str | None = None,
        capability: str,
        command_name: str,
        payload_json: str = "{}",
        result_json: str = "{}",
        success: bool = True,
    ) -> AuditEvent:
        """Create and persist an audit event row."""
        async with self._lock:
            async with self._get_session() as session:
                event = AuditEvent(
                    actor_type=actor_type,
                    actor_id=actor_id,
                    session_id=session_id,
                    capability=capability,
                    command_name=command_name,
                    payload_json=payload_json,
                    result_json=result_json,
                    success=success,
                )
                session.add(event)
                await session.commit()
                return event

    async def list_events(
        self,
        *,
        capability: str | None = None,
        limit: int = 50,
        cursor: str | None = None,
    ) -> list[AuditEvent]:
        """List audit events with optional filter and cursor pagination."""
        async with self._get_session() as session:
            stmt = select(AuditEvent)

            if capability is not None:
                stmt = stmt.where(AuditEvent.capability == capability)

            if cursor is not None:
                cursor_dt = datetime.fromisoformat(cursor)
                stmt = stmt.where(col(AuditEvent.occurred_at) < cursor_dt)

            stmt = stmt.order_by(col(AuditEvent.occurred_at).desc()).limit(limit)

            result = await session.execute(stmt)
            return list(result.scalars().all())


class ScratchRepository:
    """Scratchpad repository."""

    def __init__(self, session_factory: ClosingAwareSessionFactory) -> None:
        self._session_factory = session_factory
        self._lock = asyncio.Lock()

    def _get_session(self) -> AsyncSession:
        return self._session_factory()

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
                    scratchpad.updated_at = utc_now()
                else:
                    scratchpad = Scratch(
                        id=task_id,
                        scratch_type=ScratchType.WORKSPACE_NOTES,
                        payload={"content": content},
                    )
                    scratchpad.created_at = utc_now()
                    scratchpad.updated_at = utc_now()
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


class SessionRecordRepository:
    """Session record CRUD repository."""

    def __init__(self, session_factory: ClosingAwareSessionFactory) -> None:
        self._session_factory = session_factory
        self._lock = asyncio.Lock()

    def _get_session(self) -> AsyncSession:
        return self._session_factory()

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
                    started_at=utc_now(),
                    ended_at=None,
                )
                session.add(record)
                await session.commit()
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
                record.ended_at = utc_now()
                session.add(record)
                await session.commit()
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
                record.ended_at = utc_now()
                session.add(record)
                await session.commit()
                return record


class PlannerRepository:
    """Repository for planner proposal persistence."""

    def __init__(self, session_factory: ClosingAwareSessionFactory) -> None:
        self._session_factory = session_factory
        self._lock = asyncio.Lock()

    def _get_session(self) -> AsyncSession:
        return self._session_factory()

    async def save_proposal(
        self,
        *,
        project_id: str,
        repo_id: str | None = None,
        tasks_json: list[dict[str, Any]],
        todos_json: list[dict[str, Any]] | None = None,
    ) -> PlannerProposal:
        """Create and persist a new draft proposal."""
        async with self._lock:
            async with self._get_session() as session:
                proposal = PlannerProposal(
                    project_id=project_id,
                    repo_id=repo_id,
                    tasks_json=tasks_json,
                    todos_json=todos_json or [],
                    status=ProposalStatus.DRAFT,
                )
                session.add(proposal)
                await session.commit()
                return proposal

    async def get_proposal(self, proposal_id: str) -> PlannerProposal | None:
        """Fetch a single proposal by ID."""
        async with self._get_session() as session:
            return await session.get(PlannerProposal, proposal_id)

    async def list_pending(
        self,
        project_id: str,
        *,
        repo_id: str | None = None,
    ) -> list[PlannerProposal]:
        """List draft proposals for a project, optionally filtered by repo."""
        async with self._get_session() as session:
            stmt = select(PlannerProposal).where(
                PlannerProposal.project_id == project_id,
                PlannerProposal.status == ProposalStatus.DRAFT,
            )
            if repo_id is not None:
                stmt = stmt.where(PlannerProposal.repo_id == repo_id)
            stmt = stmt.order_by(col(PlannerProposal.created_at).desc())
            result = await session.execute(stmt)
            return list(result.scalars().all())

    async def update_status(
        self,
        proposal_id: str,
        status: ProposalStatus,
    ) -> PlannerProposal | None:
        """Transition a proposal to a new status."""
        async with self._lock:
            async with self._get_session() as session:
                proposal = await session.get(PlannerProposal, proposal_id)
                if proposal is None:
                    return None
                proposal.status = status
                proposal.updated_at = utc_now()
                session.add(proposal)
                await session.commit()
                return proposal

    async def delete_proposal(self, proposal_id: str) -> bool:
        """Delete a proposal by ID. Returns True if deleted."""
        async with self._lock:
            async with self._get_session() as session:
                proposal = await session.get(PlannerProposal, proposal_id)
                if proposal is None:
                    return False
                await session.delete(proposal)
                await session.commit()
                return True


class RepoRepository:
    """CRUD operations for Repo entities."""

    def __init__(self, session_factory: ClosingAwareSessionFactory) -> None:
        self._session_factory = session_factory
        self._lock = asyncio.Lock()

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
        async with self._lock:
            existing = await self.get_by_path(path)
            if existing:
                return existing, False
            try:
                return await self.create(path, **kwargs), True
            except IntegrityError:
                existing = await self.get_by_path(path)
                if existing is None:
                    raise
                return existing, False

    async def list_for_project(self, project_id: str) -> list[Repo]:
        """List all repos for a project via junction table."""
        async with self._get_session() as session:
            result = await session.execute(
                select(Repo)
                .join(ProjectRepo, col(ProjectRepo.repo_id) == col(Repo.id))
                .where(ProjectRepo.project_id == project_id)
                .order_by(col(ProjectRepo.display_order).asc(), col(Repo.id).asc())
            )
            return list(result.scalars().all())

    async def list_for_workspace(self, workspace_id: str) -> list[WorkspaceRepo]:
        """List all workspace-repo associations for a workspace."""
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
    ) -> ProjectRepo:
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
            return link

    async def add_to_workspace(
        self,
        workspace_id: str,
        repo_id: str,
        target_branch: str,
        worktree_path: str | None = None,
    ) -> WorkspaceRepo:
        """Add a repo to a workspace via junction table."""
        async with self._get_session() as session:
            link = WorkspaceRepo(
                workspace_id=workspace_id,
                repo_id=repo_id,
                target_branch=target_branch,
                worktree_path=worktree_path,
            )
            session.add(link)
            await session.commit()
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

    async def update_default_branch(
        self,
        repo_id: str,
        branch: str,
        *,
        mark_configured: bool = False,
    ) -> Repo | None:
        """Update Repo.default_branch, optionally marking branch as configured."""
        async with self._lock:
            async with self._get_session() as session:
                repo = await session.get(Repo, repo_id)
                if repo is None:
                    return None
                repo.default_branch = branch
                if mark_configured:
                    next_scripts = dict(repo.scripts) if repo.scripts else {}
                    next_scripts[KAGAN_BRANCH_CONFIGURED_KEY] = "true"
                    repo.scripts = next_scripts
                session.add(repo)
                await session.commit()
                return repo

    async def remove_from_workspace(self, workspace_id: str, repo_id: str) -> bool:
        """Remove a repo from a workspace. Returns True if removed."""
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
