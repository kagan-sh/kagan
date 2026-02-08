from __future__ import annotations

from typing import TYPE_CHECKING

from sqlmodel import col, select

from kagan.adapters.db.session import AsyncSessionFactory, get_session

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.adapters.db.schema import Repo, WorkspaceRepo
    from kagan.adapters.db.schema import Workspace as DbWorkspace
    from kagan.adapters.git.worktrees import GitWorktreeProtocol
    from kagan.services.projects import ProjectService
    from kagan.services.tasks import TaskService
    from kagan.services.workspaces.service import RepoWorkspaceInput


class WorkspaceGitOpsMixin:
    _session_factory: AsyncSessionFactory
    _git: GitWorktreeProtocol
    _tasks: TaskService
    _projects: ProjectService

    async def provision(
        self,
        task_id: str,
        repos: list[RepoWorkspaceInput],
        *,
        branch_name: str | None = None,
    ) -> str:
        del task_id, repos, branch_name
        raise NotImplementedError

    async def release(
        self,
        workspace_id: str,
        *,
        reason: str | None = None,
        cleanup: bool = True,
    ) -> None:
        del workspace_id, reason, cleanup
        raise NotImplementedError

    async def get_agent_working_dir(self, workspace_id: str) -> Path:
        del workspace_id
        raise NotImplementedError

    async def create(self, task_id: str, base_branch: str = "main") -> Path:
        """Create a workspace for the task and return primary worktree path."""
        from kagan.services.workspaces.service import RepoWorkspaceInput

        task = await self._tasks.get_task(task_id)
        if task is None:
            raise ValueError(f"Task {task_id} not found")

        repos = await self._projects.get_project_repos(task.project_id)
        if not repos:
            raise ValueError(f"Project {task.project_id} has no repos")

        repo_inputs = [
            RepoWorkspaceInput(
                repo_id=repo.id,
                repo_path=repo.path,
                target_branch=repo.default_branch or base_branch,
            )
            for repo in repos
        ]
        workspace_id = await self.provision(task_id, repo_inputs)
        return await self.get_agent_working_dir(workspace_id)

    async def delete(self, task_id: str, *, delete_branch: bool = False) -> None:
        del delete_branch
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            return
        await self.release(workspace.id, cleanup=True)

    async def get_path(self, task_id: str) -> Path | None:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            return None
        return await self.get_agent_working_dir(workspace.id)

    async def get_commit_log(self, task_id: str, base_branch: str = "main") -> list[str]:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            return []
        repo_rows = await self._get_workspace_repo_rows(workspace.id)
        commits: list[str] = []
        for workspace_repo, repo in repo_rows:
            if not workspace_repo.worktree_path:
                continue
            target_branch = workspace_repo.target_branch or base_branch
            repo_commits = await self._git.get_commit_log(
                workspace_repo.worktree_path,
                target_branch,
            )
            commits.extend([f"[{repo.name}] {commit}" for commit in repo_commits])
        return commits

    async def get_diff(self, task_id: str, base_branch: str = "main") -> str:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            return ""
        repo_rows = await self._get_workspace_repo_rows(workspace.id)
        chunks: list[str] = []
        for workspace_repo, repo in repo_rows:
            if not workspace_repo.worktree_path:
                continue
            target_branch = workspace_repo.target_branch or base_branch
            diff = await self._git.get_diff(workspace_repo.worktree_path, target_branch)
            if not diff.strip():
                continue
            chunks.append(f"# === {repo.name} ({target_branch}) ===")
            chunks.append(diff.rstrip())
            chunks.append("")
        return "\n".join(chunks).strip()

    async def get_diff_stats(self, task_id: str, base_branch: str = "main") -> str:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            return ""
        repo_rows = await self._get_workspace_repo_rows(workspace.id)
        summary_lines: list[str] = []
        total_files = 0
        total_insertions = 0
        total_deletions = 0
        for workspace_repo, repo in repo_rows:
            if not workspace_repo.worktree_path:
                continue
            target_branch = workspace_repo.target_branch or base_branch
            stats = await self._git.get_diff_stats(
                workspace_repo.worktree_path,
                target_branch,
            )
            files = int(stats.get("files", 0))
            insertions = int(stats.get("insertions", 0))
            deletions = int(stats.get("deletions", 0))
            total_files += files
            total_insertions += insertions
            total_deletions += deletions
            if files or insertions or deletions:
                summary_lines.append(f"{repo.name}: +{insertions} -{deletions} ({files} files)")
            else:
                summary_lines.append(f"{repo.name}: no changes")

        if not summary_lines:
            return ""
        if len(summary_lines) > 1:
            summary_lines.append(
                f"Total: +{total_insertions} -{total_deletions} ({total_files} files)"
            )
        return "\n".join(summary_lines)

    async def get_files_changed(self, task_id: str, base_branch: str = "main") -> list[str]:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            return []
        repo_rows = await self._get_workspace_repo_rows(workspace.id)
        files: list[str] = []
        for workspace_repo, repo in repo_rows:
            if not workspace_repo.worktree_path:
                continue
            target_branch = workspace_repo.target_branch or base_branch
            repo_files = await self._git.get_files_changed(
                workspace_repo.worktree_path,
                target_branch,
            )
            files.extend([f"{repo.name}:{path}" for path in repo_files])
        return files

    async def _get_workspace_repo_rows(self, workspace_id: str) -> list[tuple[WorkspaceRepo, Repo]]:
        from kagan.adapters.db.schema import Repo, WorkspaceRepo

        async with get_session(self._session_factory) as session:
            result = await session.execute(
                select(WorkspaceRepo, Repo)
                .join(Repo)
                .where(WorkspaceRepo.workspace_id == workspace_id)
                .order_by(col(WorkspaceRepo.created_at).asc())
            )
            rows = result.all()
            return [(row[0], row[1]) for row in rows]

    async def _get_workspace(self, workspace_id: str) -> DbWorkspace | None:
        from kagan.adapters.db.schema import Workspace

        async with get_session(self._session_factory) as session:
            return await session.get(Workspace, workspace_id)

    async def _get_latest_workspace_for_task(self, task_id: str) -> DbWorkspace | None:
        from kagan.adapters.db.schema import Workspace

        async with get_session(self._session_factory) as session:
            result = await session.execute(
                select(Workspace)
                .where(Workspace.task_id == task_id)
                .order_by(col(Workspace.created_at).desc())
            )
            return result.scalars().first()

    async def _get_primary_workspace_repo(self, workspace_id: str) -> WorkspaceRepo | None:
        from kagan.adapters.db.schema import ProjectRepo, Workspace, WorkspaceRepo

        async with get_session(self._session_factory) as session:
            workspace = await session.get(Workspace, workspace_id)
            if workspace is None:
                return None

            result = await session.execute(
                select(WorkspaceRepo)
                .join(ProjectRepo, col(ProjectRepo.repo_id) == col(WorkspaceRepo.repo_id))
                .where(WorkspaceRepo.workspace_id == workspace_id)
                .where(ProjectRepo.project_id == workspace.project_id)
                .order_by(
                    col(ProjectRepo.is_primary).desc(),
                    col(ProjectRepo.display_order).asc(),
                    col(WorkspaceRepo.created_at).asc(),
                )
            )
            primary = result.scalars().first()
            if primary:
                return primary

            result = await session.execute(
                select(WorkspaceRepo)
                .where(WorkspaceRepo.workspace_id == workspace_id)
                .order_by(col(WorkspaceRepo.created_at).asc())
            )
            return result.scalars().first()
