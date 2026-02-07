"""Task service interface and implementation."""

from __future__ import annotations

from typing import TYPE_CHECKING, Protocol

if TYPE_CHECKING:
    from collections.abc import Sequence

    from kagan.adapters.db.repositories import TaskRepository
    from kagan.adapters.db.schema import AgentTurn
    from kagan.core.events import EventBus
    from kagan.core.models.entities import Task
    from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
    from kagan.services.types import ProjectId, TaskId


class TaskService(Protocol):
    """Service interface for task operations."""

    async def create_task(
        self,
        title: str,
        description: str,
        *,
        project_id: ProjectId | None = None,
        created_by: str | None = None,
    ) -> Task:
        """Create a task and return the new entity."""

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
        """Update a task and return the updated entity."""

    async def set_status(
        self,
        task_id: TaskId,
        to_status: TaskStatus,
        *,
        reason: str | None = None,
    ) -> Task | None:
        """Transition a task to a new status."""

    async def get_task(self, task_id: TaskId) -> Task | None:
        """Return a task by ID."""

    async def list_tasks(
        self,
        *,
        project_id: ProjectId | None = None,
        status: TaskStatus | None = None,
    ) -> list[Task]:
        """List tasks filtered by project and/or status."""

    async def delete_task(self, task_id: TaskId) -> bool:
        """Delete a task. Returns True if deleted."""

    async def update_fields(self, task_id: TaskId, **kwargs: object) -> Task | None:
        """Update a task with keyword arguments."""

    async def move(self, task_id: TaskId, new_status: TaskStatus) -> Task | None:
        """Move a task to a new status."""

    async def mark_session_active(self, task_id: TaskId, active: bool) -> Task | None:
        """Mark a task as having an active session."""

    async def get_by_status(self, status: TaskStatus) -> Sequence[Task]:
        """Return tasks by status."""

    async def search(self, query: str) -> Sequence[Task]:
        """Search tasks by ID, title, or description."""

    async def get_scratchpad(self, task_id: TaskId) -> str:
        """Get scratchpad content for a task."""

    async def update_scratchpad(self, task_id: TaskId, content: str) -> None:
        """Update scratchpad content for a task."""

    async def clear_agent_logs(self, task_id: TaskId) -> None:
        """Delete all agent logs for a task."""

    async def append_agent_log(
        self, task_id: TaskId, log_type: str, iteration: int, content: str
    ) -> None:
        """Append an agent log entry."""

    async def get_agent_logs(self, task_id: TaskId, log_type: str) -> Sequence[AgentTurn]:
        """Get agent logs for a task by type."""

    async def append_event(self, task_id: TaskId, event_type: str, message: str) -> None:
        """Append an audit event for a task."""

    async def get_events(self, task_id: TaskId, limit: int = 20) -> Sequence[AgentTurn]:
        """Get recent audit events for a task."""

    async def increment_total_iterations(self, task_id: TaskId) -> None:
        """Increment total iteration counter."""

    async def set_review_summary(
        self, task_id: TaskId, summary: str, checks_passed: bool | None
    ) -> Task | None:
        """Set review summary and checks status."""

    async def sync_status_from_agent_complete(self, task_id: TaskId, success: bool) -> Task | None:
        """Auto-transition task when agent completes."""

    async def sync_status_from_review_pass(self, task_id: TaskId) -> Task | None:
        """Auto-transition task when review passes."""

    async def sync_status_from_review_reject(
        self, task_id: TaskId, reason: str | None = None
    ) -> Task | None:
        """Move task back to IN_PROGRESS after review rejection."""


class TaskServiceImpl:
    """Concrete TaskService backed by TaskRepository and EventBus."""

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

        return DomainTask.model_validate(updated)

    async def move(self, task_id: TaskId, new_status: TaskStatus) -> Task | None:
        return await self.set_status(task_id, new_status)

    async def mark_session_active(self, task_id: TaskId, active: bool) -> Task | None:
        return await self.update_fields(task_id, session_active=active)

    async def get_by_status(self, status: TaskStatus) -> Sequence[Task]:
        from kagan.core.models.entities import Task as DomainTask

        tasks = await self._repo.get_by_status(status)
        return [DomainTask.model_validate(task) for task in tasks]

    async def search(self, query: str) -> Sequence[Task]:
        from kagan.core.models.entities import Task as DomainTask

        tasks = await self._repo.search(query)
        return [DomainTask.model_validate(task) for task in tasks]

    async def get_scratchpad(self, task_id: TaskId) -> str:
        return await self._repo.get_scratchpad(task_id)

    async def update_scratchpad(self, task_id: TaskId, content: str) -> None:
        await self._repo.update_scratchpad(task_id, content)

    async def clear_agent_logs(self, task_id: TaskId) -> None:
        await self._repo.clear_agent_logs(task_id)

    async def append_agent_log(
        self, task_id: TaskId, log_type: str, iteration: int, content: str
    ) -> None:
        await self._repo.append_agent_log(task_id, log_type, iteration, content)

    async def get_agent_logs(self, task_id: TaskId, log_type: str) -> Sequence[AgentTurn]:
        return await self._repo.get_agent_logs(task_id, log_type)

    async def append_event(self, task_id: TaskId, event_type: str, message: str) -> None:
        await self._repo.append_event(task_id, event_type, message)

    async def get_events(self, task_id: TaskId, limit: int = 20) -> Sequence[AgentTurn]:
        return await self._repo.get_events(task_id, limit=limit)

    async def increment_total_iterations(self, task_id: TaskId) -> None:
        await self._repo.increment_total_iterations(task_id)

    async def set_review_summary(
        self, task_id: TaskId, summary: str, checks_passed: bool | None
    ) -> Task | None:
        return await self.update_fields(
            task_id, review_summary=summary, checks_passed=checks_passed
        )

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
