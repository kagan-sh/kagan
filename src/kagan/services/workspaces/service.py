"""Workspace service with multi-repo support."""

from __future__ import annotations

import contextlib
import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Protocol

from sqlmodel import col, select

from kagan.adapters.db.session import AsyncSessionFactory, get_session
from kagan.core.models.enums import WorkspaceStatus
from kagan.core.time import utc_now
from kagan.paths import get_worktree_base_dir

from .git_ops import WorkspaceGitOpsMixin
from .merge_ops import WorkspaceMergeOpsMixin

if TYPE_CHECKING:
    from kagan.adapters.db.schema import Workspace as DbWorkspace
    from kagan.adapters.db.schema import WorkspaceRepo
    from kagan.adapters.git.worktrees import GitWorktreeProtocol
    from kagan.services.projects import ProjectService
    from kagan.services.tasks import TaskService


@dataclass
class RepoWorkspaceInput:
    """Input for creating a workspace repo."""

    repo_id: str
    repo_path: str
    target_branch: str


class WorkspaceService(Protocol):
    """Protocol boundary for workspace and worktree operations."""

    async def provision(
        self,
        task_id: str,
        repos: list[RepoWorkspaceInput],
        *,
        branch_name: str | None = None,
    ) -> str: ...

    async def provision_for_project(
        self,
        task_id: str,
        project_id: str,
        *,
        branch_name: str | None = None,
    ) -> str: ...

    async def release(
        self,
        workspace_id: str,
        *,
        reason: str | None = None,
        cleanup: bool = True,
    ) -> None: ...

    async def get_workspace_repos(self, workspace_id: str) -> list[dict]: ...

    async def get_agent_working_dir(self, workspace_id: str) -> Path: ...

    async def get_workspace(self, workspace_id: str) -> DbWorkspace | None: ...

    async def list_workspaces(
        self,
        *,
        task_id: str | None = None,
        repo_id: str | None = None,
    ) -> list[DbWorkspace]: ...

    async def create(self, task_id: str, base_branch: str = "main") -> Path: ...

    async def delete(self, task_id: str, *, delete_branch: bool = False) -> None: ...

    async def get_path(self, task_id: str) -> Path | None: ...

    async def get_commit_log(self, task_id: str, base_branch: str = "main") -> list[str]: ...

    async def get_diff(self, task_id: str, base_branch: str = "main") -> str: ...

    async def get_diff_stats(self, task_id: str, base_branch: str = "main") -> str: ...

    async def get_files_changed(self, task_id: str, base_branch: str = "main") -> list[str]: ...

    async def get_merge_worktree_path(self, task_id: str, base_branch: str = "main") -> Path: ...

    async def prepare_merge_conflicts(
        self, task_id: str, base_branch: str = "main"
    ) -> tuple[bool, str]: ...

    async def cleanup_orphans(self, valid_task_ids: set[str]) -> list[str]: ...

    async def rebase_onto_base(
        self, task_id: str, base_branch: str = "main"
    ) -> tuple[bool, str, list[str]]: ...

    async def abort_rebase(self, task_id: str) -> tuple[bool, str]: ...

    async def get_files_changed_on_base(
        self, task_id: str, base_branch: str = "main"
    ) -> list[str]: ...


class WorkspaceServiceImpl(WorkspaceGitOpsMixin, WorkspaceMergeOpsMixin):
    """Implementation of multi-repo WorkspaceService."""

    def __init__(
        self,
        session_factory: AsyncSessionFactory,
        git_adapter: GitWorktreeProtocol,
        task_service: TaskService,
        project_service: ProjectService,
    ) -> None:
        self._session_factory = session_factory
        self._git = git_adapter
        self._tasks = task_service
        self._projects = project_service
        self._merge_worktrees_dir = get_worktree_base_dir() / "merge-worktrees"

    def _get_workspace_base_dir(self, workspace_id: str) -> Path:
        return get_worktree_base_dir() / "worktrees" / workspace_id

    async def provision(
        self,
        task_id: str,
        repos: list[RepoWorkspaceInput],
        *,
        branch_name: str | None = None,
    ) -> str:
        """Provision a workspace with worktrees for all repos."""
        import uuid

        from kagan.adapters.db.schema import Workspace, WorkspaceRepo

        if not repos:
            raise ValueError("At least one repo is required to provision a workspace")

        workspace_id = uuid.uuid4().hex[:8]
        branch_name = branch_name or f"kagan/{workspace_id}"

        task = await self._tasks.get_task(task_id)
        if task is None:
            raise ValueError(f"Task {task_id} not found")

        base_dir = self._get_workspace_base_dir(workspace_id)
        base_dir.mkdir(parents=True, exist_ok=True)

        workspace = Workspace(
            id=workspace_id,
            project_id=task.project_id,
            task_id=task_id,
            path=str(base_dir),
            branch_name=branch_name,
        )

        created_paths: list[Path] = []
        workspace_repos: list[WorkspaceRepo] = []
        try:
            for repo_input in repos:
                worktree_path = base_dir / Path(repo_input.repo_path).name
                await self._git.create_worktree(
                    repo_path=repo_input.repo_path,
                    worktree_path=str(worktree_path),
                    branch_name=branch_name,
                    base_branch=repo_input.target_branch,
                )
                created_paths.append(worktree_path)

                workspace_repos.append(
                    WorkspaceRepo(
                        workspace_id=workspace_id,
                        repo_id=repo_input.repo_id,
                        target_branch=repo_input.target_branch,
                        worktree_path=str(worktree_path),
                    )
                )

            async with get_session(self._session_factory) as session:
                session.add(workspace)
                for wr in workspace_repos:
                    session.add(wr)
                await session.commit()

        except Exception:
            for path in created_paths:
                with contextlib.suppress(Exception):
                    await self._git.delete_worktree(str(path))
            shutil.rmtree(base_dir, ignore_errors=True)
            raise

        return workspace_id

    async def provision_for_project(
        self,
        task_id: str,
        project_id: str,
        *,
        branch_name: str | None = None,
    ) -> str:
        """Provision workspace using all project repos."""
        from kagan.adapters.db.schema import ProjectRepo, Repo

        async with get_session(self._session_factory) as session:
            result = await session.execute(
                select(ProjectRepo, Repo)
                .join(Repo)
                .where(ProjectRepo.project_id == project_id)
                .order_by(col(ProjectRepo.display_order))
            )
            project_repos = result.all()

        if not project_repos:
            raise ValueError(f"Project {project_id} has no repos")

        repos = [
            RepoWorkspaceInput(
                repo_id=repo.id,
                repo_path=repo.path,
                target_branch=repo.default_branch,
            )
            for project_repo, repo in project_repos
        ]

        return await self.provision(task_id, repos, branch_name=branch_name)

    async def release(
        self,
        workspace_id: str,
        *,
        reason: str | None = None,
        cleanup: bool = True,
    ) -> None:
        """Release workspace and clean up worktrees."""
        from kagan.adapters.db.schema import Workspace, WorkspaceRepo

        async with get_session(self._session_factory) as session:
            result = await session.execute(select(Workspace).where(Workspace.id == workspace_id))
            workspace = result.scalars().first()

            if not workspace:
                raise ValueError(f"Workspace {workspace_id} not found")

            if cleanup:
                result = await session.execute(
                    select(WorkspaceRepo).where(WorkspaceRepo.workspace_id == workspace_id)
                )
                workspace_repos = result.scalars().all()

                for wr in workspace_repos:
                    if wr.worktree_path and Path(wr.worktree_path).exists():
                        with contextlib.suppress(Exception):
                            await self._git.delete_worktree(wr.worktree_path)

                if workspace.path and Path(workspace.path).exists():
                    shutil.rmtree(workspace.path, ignore_errors=True)

            workspace.status = WorkspaceStatus.ARCHIVED
            workspace.updated_at = utc_now()
            session.add(workspace)
            await session.commit()

    async def get_workspace_repos(self, workspace_id: str) -> list[dict]:
        """Get all repos for a workspace with paths and status."""
        from kagan.adapters.db.schema import Repo, WorkspaceRepo

        async with get_session(self._session_factory) as session:
            result = await session.execute(
                select(WorkspaceRepo, Repo)
                .join(Repo)
                .where(WorkspaceRepo.workspace_id == workspace_id)
            )
            results = result.all()

        items: list[dict] = []
        for workspace_repo, repo in results:
            diff_stats = None
            has_changes = False
            if workspace_repo.worktree_path:
                has_uncommitted = await self._git.has_uncommitted_changes(
                    workspace_repo.worktree_path
                )
                diff_stats = await self._git.get_diff_stats(
                    workspace_repo.worktree_path,
                    workspace_repo.target_branch,
                )
                diff_files = int(diff_stats.get("files", 0)) if diff_stats else 0
                diff_insertions = int(diff_stats.get("insertions", 0)) if diff_stats else 0
                diff_deletions = int(diff_stats.get("deletions", 0)) if diff_stats else 0
                has_changes = bool(
                    has_uncommitted or diff_files or diff_insertions or diff_deletions
                )
            item = {
                "repo_id": repo.id,
                "repo_name": repo.name,
                "repo_path": repo.path,
                "worktree_path": workspace_repo.worktree_path,
                "target_branch": workspace_repo.target_branch,
                "has_changes": has_changes,
                "diff_stats": diff_stats,
            }
            items.append(item)

        return items

    async def get_agent_working_dir(self, workspace_id: str) -> Path:
        """Get working directory for agents (primary repo's worktree)."""
        primary_repo = await self._get_primary_workspace_repo(workspace_id)
        if primary_repo is None or not primary_repo.worktree_path:
            raise ValueError(f"Workspace {workspace_id} has no repos")
        return Path(primary_repo.worktree_path)

    async def get_workspace(self, workspace_id: str) -> DbWorkspace | None:
        workspace = await self._get_workspace(workspace_id)
        return workspace

    async def list_workspaces(
        self,
        *,
        task_id: str | None = None,
        repo_id: str | None = None,
    ) -> list[DbWorkspace]:
        from kagan.adapters.db.schema import Workspace, WorkspaceRepo

        async with get_session(self._session_factory) as session:
            statement = select(Workspace).order_by(col(Workspace.created_at).desc())
            if task_id is not None:
                statement = statement.where(Workspace.task_id == task_id)
            if repo_id is not None:
                statement = (
                    statement.join(WorkspaceRepo).where(WorkspaceRepo.repo_id == repo_id).distinct()
                )
            result = await session.execute(statement)
            return list(result.scalars().all())

    async def cleanup_orphans(self, valid_task_ids: set[str]) -> list[str]:
        from kagan.adapters.db.schema import Workspace

        async with get_session(self._session_factory) as session:
            result = await session.execute(select(Workspace))
            workspaces = result.scalars().all()

        cleaned: list[str] = []
        for workspace in workspaces:
            if workspace.task_id and workspace.task_id not in valid_task_ids:
                await self.release(workspace.id, cleanup=True)
                cleaned.append(workspace.id)

        return cleaned
