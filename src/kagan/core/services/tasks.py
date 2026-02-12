"""Task service implementation."""

from __future__ import annotations

import re
from typing import TYPE_CHECKING, Protocol

from kagan.core.models.enums import (
    SessionStatus,
    SessionType,
    transition_status_from_agent_complete,
    transition_status_from_review_pass,
    transition_status_from_review_reject,
)

if TYPE_CHECKING:
    from collections.abc import Sequence

    from kagan.core.adapters.db.repositories import TaskRepository
    from kagan.core.adapters.db.repositories.auxiliary import (
        ScratchRepository,
        SessionRecordRepository,
    )
    from kagan.core.adapters.db.schema import Session, Task
    from kagan.core.events import EventBus
    from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
    from kagan.core.services.types import ProjectId, TaskId


_TASK_MENTION_RE = re.compile(r"@([A-Za-z0-9]{8})")


def _extract_task_mentions(description: str) -> set[str]:
    if not description:
        return set()
    return {match.group(1) for match in _TASK_MENTION_RE.finditer(description)}


class TaskService(Protocol):
    """Protocol boundary for task operations."""

    async def create_task(
        self,
        title: str,
        description: str,
        *,
        project_id: ProjectId | None = None,
        created_by: str | None = None,
    ) -> Task: ...

    async def update_task(
        self,
        task_id: TaskId,
        *,
        title: str | None = None,
        description: str | None = None,
        priority: TaskPriority | None = None,
        task_type: TaskType | None = None,
        agent_backend: str | None = None,
        acceptance_criteria: list[str] | None = None,
    ) -> Task | None: ...

    async def set_status(
        self,
        task_id: TaskId,
        to_status: TaskStatus,
        *,
        reason: str | None = None,
    ) -> Task | None: ...

    async def get_task(self, task_id: TaskId) -> Task | None: ...

    async def list_tasks(
        self,
        *,
        project_id: ProjectId | None = None,
        status: TaskStatus | None = None,
    ) -> list[Task]: ...

    async def delete_task(self, task_id: TaskId) -> bool: ...

    async def update_fields(self, task_id: TaskId, **kwargs: object) -> Task | None: ...

    async def move(self, task_id: TaskId, new_status: TaskStatus) -> Task | None: ...

    async def create_session_record(
        self,
        *,
        workspace_id: str,
        session_type: SessionType,
        external_id: str | None = None,
    ) -> Session: ...

    async def close_session_record(
        self,
        session_id: str,
        *,
        status: SessionStatus = SessionStatus.CLOSED,
    ) -> Session | None: ...

    async def close_session_by_external_id(
        self,
        external_id: str,
        *,
        status: SessionStatus = SessionStatus.CLOSED,
    ) -> Session | None: ...

    async def get_by_status(self, status: TaskStatus) -> Sequence[Task]: ...

    async def search(self, query: str) -> Sequence[Task]: ...

    async def get_task_links(self, task_id: TaskId) -> list[str]: ...

    async def get_scratchpad(self, task_id: TaskId) -> str: ...

    async def update_scratchpad(self, task_id: TaskId, content: str) -> None: ...

    async def sync_status_from_agent_complete(
        self, task_id: TaskId, success: bool
    ) -> Task | None: ...

    async def sync_status_from_review_pass(self, task_id: TaskId) -> Task | None: ...

    async def sync_status_from_review_reject(
        self, task_id: TaskId, reason: str | None = None
    ) -> Task | None: ...


class TaskServiceImpl:
    """Task service backed by TaskRepository and EventBus."""

    def __init__(
        self,
        repo: TaskRepository,
        event_bus: EventBus,
        *,
        session_repo: SessionRecordRepository | None = None,
        scratch_repo: ScratchRepository | None = None,
    ) -> None:
        self._repo = repo
        self._events = event_bus
        if session_repo is None:
            from kagan.core.adapters.db.repositories.auxiliary import SessionRecordRepository

            session_repo = SessionRecordRepository(repo.session_factory)
        if scratch_repo is None:
            from kagan.core.adapters.db.repositories.auxiliary import ScratchRepository

            scratch_repo = ScratchRepository(repo.session_factory)
        self._sessions = session_repo
        self._scratch = scratch_repo

    async def create_task(
        self,
        title: str,
        description: str,
        *,
        project_id: ProjectId | None = None,
        created_by: str | None = None,
    ) -> Task:
        from kagan.core.adapters.db.schema import Task as DbTask
        from kagan.core.events import TaskCreated

        project_id = project_id or self._repo.default_project_id
        if project_id is None:
            raise ValueError("Project ID is required to create a task")

        db_task = DbTask(
            project_id=project_id,
            title=title,
            description=description,
        )
        created = await self._repo.create(db_task)
        await self._events.publish(
            TaskCreated(
                task_id=created.id or "",
                status=created.status,
                title=created.title,
                created_at=created.created_at,
            )
        )
        await self._sync_task_links(created.id, created.project_id, created.description)
        return created

    async def update_task(
        self,
        task_id: TaskId,
        *,
        title: str | None = None,
        description: str | None = None,
        priority: TaskPriority | None = None,
        task_type: TaskType | None = None,
        agent_backend: str | None = None,
        acceptance_criteria: list[str] | None = None,
    ) -> Task | None:
        return await self.update_fields(
            task_id,
            title=title,
            description=description,
            priority=priority,
            task_type=task_type,
            agent_backend=agent_backend,
            acceptance_criteria=acceptance_criteria,
        )

    async def set_status(
        self,
        task_id: TaskId,
        to_status: TaskStatus,
        *,
        reason: str | None = None,
    ) -> Task | None:
        from kagan.core.events import TaskStatusChanged, TaskUpdated

        current = await self._repo.get(task_id)
        if current is None:
            return None
        updated = await self._repo.update(task_id, status=to_status)
        if updated is None:
            return None
        await self._events.publish(
            TaskStatusChanged(
                task_id=task_id,
                from_status=current.status,
                to_status=updated.status,
                reason=reason,
                updated_at=updated.updated_at,
            )
        )
        await self._events.publish(
            TaskUpdated(task_id=task_id, fields_changed=["status"], updated_at=updated.updated_at)
        )
        return updated

    async def get_task(self, task_id: TaskId) -> Task | None:
        task = await self._repo.get(task_id)
        return task

    async def list_tasks(
        self,
        *,
        project_id: ProjectId | None = None,
        status: TaskStatus | None = None,
    ) -> list[Task]:
        if status:
            tasks = await self._repo.get_by_status(status, project_id=project_id)
        else:
            tasks = await self._repo.get_all(project_id=project_id)
        return list(tasks)

    async def delete_task(self, task_id: TaskId) -> bool:
        from kagan.core.events import TaskDeleted

        deleted = await self._repo.delete(task_id)
        if deleted:
            await self._events.publish(TaskDeleted(task_id=task_id))
        return deleted

    async def update_fields(self, task_id: TaskId, **kwargs: object) -> Task | None:
        from kagan.core.events import TaskStatusChanged, TaskUpdated

        current = await self._repo.get(task_id)
        if current is None:
            return None
        updated = await self._repo.update(task_id, **kwargs)
        if updated is None:
            return None

        changed_fields = list(kwargs.keys())
        await self._events.publish(
            TaskUpdated(
                task_id=task_id,
                fields_changed=changed_fields,
                updated_at=updated.updated_at,
            )
        )

        if "status" in kwargs and kwargs["status"] is not None and current.status != updated.status:
            await self._events.publish(
                TaskStatusChanged(
                    task_id=task_id,
                    from_status=current.status,
                    to_status=updated.status,
                    reason=None,
                    updated_at=updated.updated_at,
                )
            )

        if "description" in kwargs and kwargs["description"] is not None:
            await self._sync_task_links(updated.id, updated.project_id, updated.description)

        return updated

    async def move(self, task_id: TaskId, new_status: TaskStatus) -> Task | None:
        return await self.set_status(task_id, new_status)

    async def create_session_record(
        self,
        *,
        workspace_id: str,
        session_type: SessionType,
        external_id: str | None = None,
    ) -> Session:
        return await self._sessions.create_session_record(
            workspace_id=workspace_id,
            session_type=session_type,
            external_id=external_id,
        )

    async def close_session_record(
        self,
        session_id: str,
        *,
        status: SessionStatus = SessionStatus.CLOSED,
    ) -> Session | None:
        return await self._sessions.close_session_record(session_id, status=status)

    async def close_session_by_external_id(
        self,
        external_id: str,
        *,
        status: SessionStatus = SessionStatus.CLOSED,
    ) -> Session | None:
        return await self._sessions.close_session_by_external_id(external_id, status=status)

    async def get_by_status(self, status: TaskStatus) -> Sequence[Task]:
        tasks = await self._repo.get_by_status(status)
        return tasks

    async def search(self, query: str) -> Sequence[Task]:
        tasks = await self._repo.search(query)
        return tasks

    async def get_task_links(self, task_id: TaskId) -> list[str]:
        return await self._repo.get_task_links(task_id)

    async def get_scratchpad(self, task_id: TaskId) -> str:
        return await self._scratch.get_scratchpad(task_id)

    async def update_scratchpad(self, task_id: TaskId, content: str) -> None:
        await self._scratch.update_scratchpad(task_id, content)

    async def sync_status_from_agent_complete(self, task_id: TaskId, success: bool) -> Task | None:
        task = await self._repo.get(task_id)
        if task is None:
            return None
        next_status = transition_status_from_agent_complete(task.status, success)
        if next_status == task.status:
            return task
        task = await self.set_status(task_id, next_status, reason="agent_complete")
        return task

    async def sync_status_from_review_pass(self, task_id: TaskId) -> Task | None:
        task = await self._repo.get(task_id)
        if task is None:
            return None
        next_status = transition_status_from_review_pass(task.status)
        if next_status == task.status:
            return task
        task = await self.set_status(task_id, next_status, reason="review_passed")
        return task

    async def sync_status_from_review_reject(
        self, task_id: TaskId, reason: str | None = None
    ) -> Task | None:
        task = await self._repo.get(task_id)
        if task is None:
            return None
        next_status = transition_status_from_review_reject(task.status)
        if next_status == task.status:
            return task
        task = await self.set_status(task_id, next_status, reason=reason)
        return task

    async def _sync_task_links(self, task_id: str, project_id: str, description: str) -> None:
        mentions = _extract_task_mentions(description)
        if not mentions:
            await self._repo.replace_task_links(task_id, set())
            return

        tasks = await self._repo.get_tasks_by_ids(mentions, project_id=project_id)
        valid_ids = {task.id for task in tasks if task.id != task_id}
        await self._repo.replace_task_links(task_id, valid_ids)
