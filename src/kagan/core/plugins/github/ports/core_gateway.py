"""Core-facing port for GitHub plugin use cases."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Protocol

if TYPE_CHECKING:
    from kagan.core.adapters.db.schema import Project, Repo, Task, Workspace


class GitHubCoreGateway(Protocol):
    """Port for core data and mutation operations used by GitHub use cases."""

    async def get_project(self, project_id: str) -> Project | None:
        """Fetch a project by ID."""
        ...

    async def get_project_repos(self, project_id: str) -> list[Repo]:
        """List project repos by display order."""
        ...

    async def get_task(self, task_id: str) -> Task | None:
        """Fetch a task by ID."""
        ...

    async def create_task(self, *, title: str, description: str, project_id: str) -> Task:
        """Create a task projection for GitHub issue sync."""
        ...

    async def update_task_fields(self, task_id: str, **fields: Any) -> None:
        """Update mutable task fields."""
        ...

    async def list_workspaces(self, *, task_id: str) -> list[Workspace]:
        """List workspaces for a task."""
        ...

    async def get_workspace_repos(self, workspace_id: str) -> list[dict[str, Any]]:
        """List workspace repo rows for a workspace."""
        ...

    async def update_repo_scripts(self, repo_id: str, updates: dict[str, str]) -> None:
        """Merge script key/value updates into repo state."""
        ...


__all__ = ["GitHubCoreGateway"]
