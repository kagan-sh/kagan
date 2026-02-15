"""AppContext-backed implementation of GitHub core gateway port."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from kagan.core.adapters.db.schema import Project, Repo, Task, Workspace
    from kagan.core.bootstrap import AppContext


class AppContextCoreGateway:
    """Bridge from GitHub use cases to core services."""

    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    async def get_project(self, project_id: str) -> Project | None:
        return await self._ctx.project_service.get_project(project_id)

    async def get_project_repos(self, project_id: str) -> list[Repo]:
        return await self._ctx.project_service.get_project_repos(project_id)

    async def get_task(self, task_id: str) -> Task | None:
        return await self._ctx.task_service.get_task(task_id)

    async def create_task(self, *, title: str, description: str, project_id: str) -> Task:
        return await self._ctx.task_service.create_task(
            title=title,
            description=description,
            project_id=project_id,
        )

    async def update_task_fields(self, task_id: str, **fields: Any) -> None:
        await self._ctx.task_service.update_fields(task_id, **fields)

    async def list_workspaces(self, *, task_id: str) -> list[Workspace]:
        return await self._ctx.workspace_service.list_workspaces(task_id=task_id)

    async def get_workspace_repos(self, workspace_id: str) -> list[dict[str, Any]]:
        return await self._ctx.workspace_service.get_workspace_repos(workspace_id)

    async def update_repo_scripts(self, repo_id: str, updates: dict[str, str]) -> None:
        await self._ctx.project_service.update_repo_script_values(repo_id, updates)


__all__ = ["AppContextCoreGateway"]
