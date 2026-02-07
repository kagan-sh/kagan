"""Task service implementation."""

from __future__ import annotations

import re
from typing import TYPE_CHECKING

from kagan.core.models.enums import SessionStatus, SessionType

if TYPE_CHECKING:
    from collections.abc import Sequence

    from kagan.adapters.db.repositories import TaskRepository
    from kagan.adapters.db.schema import Session
    from kagan.core.events import EventBus
    from kagan.core.models.entities import Task
    from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
    from kagan.services.types import ProjectId, TaskId


_TASK_MENTION_RE = re.compile(r"@([A-Za-z0-9]{8})")


def _extract_task_mentions(description: str) -> set[str]:
    if not description:
        return set()
    return {match.group(1) for match in _TASK_MENTION_RE.finditer(description)}


class TaskService:
    """Task service backed by TaskRepository and EventBus."""

    def __init__(self, repo: TaskRepository, event_bus: EventBus) -> None:
        self._repo = repo
        self._events = event_bus

    async def create_task(
        self,
        title: str,
        description: str,
        *,
        project_id: ProjectId | None = None,
        created_by: str | None = None,
    ) -> Task:
        from kagan.adapters.db.schema import Task as DbTask
        from kagan.core.events import TaskCreated
        from kagan.core.models.entities import Task as DomainTask

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
        return DomainTask.model_validate(created)

    async def update_task(
        self,
        task_id: TaskId,
        *,
        title: str | None = None,
        description: str | None = None,
        priority: TaskPriority | None = None,
        task_type: TaskType | None = None,
        assigned_hat: str | None = None,
        agent_backend: str | None = None,
        acceptance_criteria: list[str] | None = None,
    ) -> Task | None:
        return await self.update_fields(
            task_id,
            title=title,
            description=description,
            priority=priority,
            task_type=task_type,
            assigned_hat=assigned_hat,
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
        from kagan.core.models.entities import Task as DomainTask

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
        return DomainTask.model_validate(updated)

    async def get_task(self, task_id: TaskId) -> Task | None:
        from kagan.core.models.entities import Task as DomainTask

        task = await self._repo.get(task_id)
        return DomainTask.model_validate(task) if task else None

    async def list_tasks(
        self,
        *,
        project_id: ProjectId | None = None,
        status: TaskStatus | None = None,
    ) -> list[Task]:
        from kagan.core.models.entities import Task as DomainTask

        if status:
            tasks = await self._repo.get_by_status(status, project_id=project_id)
        else:
            tasks = await self._repo.get_all(project_id=project_id)
        return [DomainTask.model_validate(task) for task in tasks]

    async def delete_task(self, task_id: TaskId) -> bool:
        return await self._repo.delete(task_id)

    async def update_fields(self, task_id: TaskId, **kwargs: object) -> Task | None:
        from kagan.core.events import TaskStatusChanged, TaskUpdated
        from kagan.core.models.entities import Task as DomainTask

        current = await self._repo.get(task_id)
        if current is None:
            return None
        updated = await self._repo.update(task_id, **kwargs)
        if updated is None:
            return None

        changed_fields = [key for key, value in kwargs.items() if value is not None]
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

        return DomainTask.model_validate(updated)

    async def move(self, task_id: TaskId, new_status: TaskStatus) -> Task | None:
        return await self.set_status(task_id, new_status)

    async def create_session_record(
        self,
        *,
        workspace_id: str,
        session_type: SessionType,
        external_id: str | None = None,
    ) -> Session:
        return await self._repo.create_session_record(
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
        return await self._repo.close_session_record(session_id, status=status)

    async def close_session_by_external_id(
        self,
        external_id: str,
        *,
        status: SessionStatus = SessionStatus.CLOSED,
    ) -> Session | None:
        return await self._repo.close_session_by_external_id(external_id, status=status)

    async def get_by_status(self, status: TaskStatus) -> Sequence[Task]:
        from kagan.core.models.entities import Task as DomainTask

        tasks = await self._repo.get_by_status(status)
        return [DomainTask.model_validate(task) for task in tasks]

    async def search(self, query: str) -> Sequence[Task]:
        from kagan.core.models.entities import Task as DomainTask

        tasks = await self._repo.search(query)
        return [DomainTask.model_validate(task) for task in tasks]

    async def get_task_links(self, task_id: TaskId) -> list[str]:
        return await self._repo.get_task_links(task_id)

    async def get_scratchpad(self, task_id: TaskId) -> str:
        return await self._repo.get_scratchpad(task_id)

    async def update_scratchpad(self, task_id: TaskId, content: str) -> None:
        await self._repo.update_scratchpad(task_id, content)

    async def sync_status_from_agent_complete(self, task_id: TaskId, success: bool) -> Task | None:
        from kagan.core.models.entities import Task as DomainTask

        task = await self._repo.sync_status_from_agent_complete(task_id, success)
        return DomainTask.model_validate(task) if task else None

    async def sync_status_from_review_pass(self, task_id: TaskId) -> Task | None:
        from kagan.core.models.entities import Task as DomainTask

        task = await self._repo.sync_status_from_review_pass(task_id)
        return DomainTask.model_validate(task) if task else None

    async def sync_status_from_review_reject(
        self, task_id: TaskId, reason: str | None = None
    ) -> Task | None:
        from kagan.core.models.entities import Task as DomainTask

        task = await self._repo.sync_status_from_review_reject(task_id, reason=reason)
        return DomainTask.model_validate(task) if task else None

    async def _sync_task_links(self, task_id: str, project_id: str, description: str) -> None:
        mentions = _extract_task_mentions(description)
        if not mentions:
            await self._repo.replace_task_links(task_id, set())
            return

        tasks = await self._repo.get_tasks_by_ids(mentions, project_id=project_id)
        valid_ids = {task.id for task in tasks if task.id != task_id}
        await self._repo.replace_task_links(task_id, valid_ids)
