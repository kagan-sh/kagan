"""Workspace service interface and implementation."""

from __future__ import annotations

from datetime import datetime
from typing import TYPE_CHECKING, Protocol

from kagan.adapters.git.worktrees import WorktreeManager
from kagan.core.models.entities import Workspace as DomainWorkspace

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.config import KaganConfig
    from kagan.core.models.entities import Workspace
    from kagan.services.tasks import TaskService
    from kagan.services.types import RepoId, TaskId, WorkspaceId


class WorkspaceService(Protocol):
    """Service interface for workspace operations."""

    async def provision(
        self,
        task_id: TaskId | None,
        *,
        repo_id: RepoId | None = None,
    ) -> Workspace:
        """Create a workspace for a task or generic repo context."""

    async def release(self, workspace_id: WorkspaceId, *, reason: str | None = None) -> None:
        """Release a workspace and clean up resources."""

    async def get_workspace(self, workspace_id: WorkspaceId) -> Workspace | None:
        """Return a workspace by ID."""

    async def list_workspaces(
        self,
        *,
        task_id: TaskId | None = None,
        repo_id: RepoId | None = None,
    ) -> list[Workspace]:
        """List workspaces filtered by task or repo."""

    async def create(self, task_id: TaskId, title: str, base_branch: str = "main") -> Path:
        """Create a worktree for the task and return its path."""

    async def delete(self, task_id: TaskId, *, delete_branch: bool = False) -> None:
        """Delete a worktree and optionally its branch."""

    async def get_path(self, task_id: TaskId) -> Path | None:
        """Return the worktree path, if it exists."""

    async def get_branch_name(self, task_id: TaskId) -> str | None:
        """Return the branch name for a worktree."""

    async def get_commit_log(self, task_id: TaskId, base_branch: str = "main") -> list[str]:
        """Return commit messages for the task worktree."""

    async def get_diff(self, task_id: TaskId, base_branch: str = "main") -> str:
        """Return the diff between task branch and base."""

    async def get_diff_stats(self, task_id: TaskId, base_branch: str = "main") -> str:
        """Return diff stats for the task branch."""

    async def get_files_changed(self, task_id: TaskId, base_branch: str = "main") -> list[str]:
        """Return files changed by the task branch."""

    async def get_merge_worktree_path(self, base_branch: str = "main") -> Path:
        """Return the merge worktree path, creating it if needed."""

    async def prepare_merge_conflicts(
        self, task_id: TaskId, base_branch: str = "main"
    ) -> tuple[bool, str]:
        """Prepare merge worktree for manual conflict resolution."""

    async def cleanup_orphans(self, valid_task_ids: set[TaskId]) -> list[str]:
        """Remove worktrees not associated with any known task."""


class WorkspaceServiceImpl(WorktreeManager):
    """Concrete WorkspaceService backed by git worktrees."""

    def __init__(self, repo_root: Path, task_service: TaskService, config: KaganConfig) -> None:
        super().__init__(repo_root=repo_root)
        self._tasks = task_service
        self._config = config

    async def provision(
        self,
        task_id: TaskId | None,
        *,
        repo_id: RepoId | None = None,
    ) -> Workspace:
        if task_id is None:
            raise ValueError("task_id is required to provision a workspace")
        task = await self._tasks.get_task(task_id)
        if task is None:
            raise ValueError(f"Task not found: {task_id}")

        worktree_path = await self.create(
            task_id, task.title, self._config.general.default_base_branch
        )
        branch_name = await self.get_branch_name(task_id) or f"kagan/{task_id}"
        return DomainWorkspace(
            id=task_id,
            project_id=task.project_id,
            repo_id=repo_id or task.repo_id or "",
            task_id=task_id,
            branch_name=branch_name,
            path=str(worktree_path),
            created_at=datetime.now(),
            updated_at=datetime.now(),
        )

    async def release(self, workspace_id: WorkspaceId, *, reason: str | None = None) -> None:
        del reason
        await self.delete(workspace_id, delete_branch=True)

    async def get_workspace(self, workspace_id: WorkspaceId) -> Workspace | None:
        path = await self.get_path(workspace_id)
        if path is None:
            return None
        task = await self._tasks.get_task(workspace_id)
        if task is None:
            return None
        branch_name = await self.get_branch_name(workspace_id) or f"kagan/{workspace_id}"
        return DomainWorkspace(
            id=workspace_id,
            project_id=task.project_id,
            repo_id=task.repo_id or "",
            task_id=task.id,
            branch_name=branch_name,
            path=str(path),
            created_at=task.created_at,
            updated_at=task.updated_at,
        )

    async def list_workspaces(
        self,
        *,
        task_id: TaskId | None = None,
        repo_id: RepoId | None = None,
    ) -> list[Workspace]:
        del repo_id
        workspace_ids = [task_id] if task_id else await self.list_all()
        workspaces: list[DomainWorkspace] = []
        for workspace_id in workspace_ids:
            workspace = await self.get_workspace(workspace_id)
            if workspace:
                workspaces.append(workspace)
        return workspaces
