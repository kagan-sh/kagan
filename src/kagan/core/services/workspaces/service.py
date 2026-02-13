"""Workspace service with multi-repo support."""

from __future__ import annotations

import contextlib
import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Protocol

from sqlmodel import col, select

from kagan.core.adapters.db.session import AsyncSessionFactory, get_session
from kagan.core.models.enums import WorkspaceStatus
from kagan.core.paths import get_worktree_base_dir
from kagan.core.time import utc_now

from .merge_ops import WorkspaceMergeOpsMixin

if TYPE_CHECKING:
    from kagan.core.adapters.db.schema import Repo, WorkspaceRepo
    from kagan.core.adapters.db.schema import Workspace as DbWorkspace
    from kagan.core.adapters.git.worktrees import GitWorktreeProtocol
    from kagan.core.services.projects import ProjectService
    from kagan.core.services.tasks import TaskService


@dataclass
class RepoWorkspaceInput:
    """Input for creating a workspace repo."""

    repo_id: str
    repo_path: str
    target_branch: str


@dataclass
class JanitorResult:
    """Result of janitor cleanup operations."""

    worktrees_pruned: int
    branches_deleted: list[str]
    repos_processed: list[str]

    @property
    def total_cleaned(self) -> int:
        """Total items cleaned up."""
        return self.worktrees_pruned + len(self.branches_deleted)


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

    async def create(self, task_id: str, base_branch: str | None = None) -> Path: ...

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

    async def run_janitor(
        self,
        valid_workspace_ids: set[str],
        *,
        prune_worktrees: bool = True,
        gc_branches: bool = True,
    ) -> JanitorResult: ...

    async def rebase_onto_base(
        self, task_id: str, base_branch: str = "main"
    ) -> tuple[bool, str, list[str]]: ...

    async def abort_rebase(self, task_id: str) -> tuple[bool, str]: ...

    async def get_files_changed_on_base(
        self, task_id: str, base_branch: str = "main"
    ) -> list[str]: ...


class WorkspaceServiceImpl(WorkspaceMergeOpsMixin):
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

    # ------------------------------------------------------------------
    # Provisioning and lifecycle
    # ------------------------------------------------------------------

    async def provision(
        self,
        task_id: str,
        repos: list[RepoWorkspaceInput],
        *,
        branch_name: str | None = None,
    ) -> str:
        """Provision a workspace with worktrees for all repos."""
        import uuid

        from kagan.core.adapters.db.schema import Workspace, WorkspaceRepo

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
        from kagan.core.adapters.db.schema import ProjectRepo, Repo

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
        from kagan.core.adapters.db.schema import Workspace, WorkspaceRepo

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
        from kagan.core.adapters.db.schema import Repo, WorkspaceRepo

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
        from kagan.core.adapters.db.schema import Workspace, WorkspaceRepo

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
        from kagan.core.adapters.db.schema import Workspace

        async with get_session(self._session_factory) as session:
            result = await session.execute(select(Workspace))
            workspaces = result.scalars().all()

        cleaned: list[str] = []
        for workspace in workspaces:
            if workspace.task_id and workspace.task_id not in valid_task_ids:
                await self.release(workspace.id, cleanup=True)
                cleaned.append(workspace.id)

        return cleaned

    async def run_janitor(
        self,
        valid_workspace_ids: set[str],
        *,
        prune_worktrees: bool = True,
        gc_branches: bool = True,
    ) -> JanitorResult:
        """Run janitor cleanup for stale worktrees and orphan kagan/* branches.

        This performs two cleanup operations:

        1. **Worktree pruning**: Runs `git worktree prune` on all project repos
           to clean up stale worktree administrative files for worktrees that
           no longer exist on disk.

        2. **Branch GC**: Deletes local `kagan/*` branches that are no longer
           associated with an active workspace. Only deletes branches that:
           - Match the `kagan/*` pattern (managed branches)
           - Are not currently checked out in any worktree
           - Do not belong to an active workspace in valid_workspace_ids

        Args:
            valid_workspace_ids: Set of workspace IDs that are still active.
                Branches matching these IDs will be preserved.
            prune_worktrees: If True, run git worktree prune on all repos.
            gc_branches: If True, delete orphaned kagan/* branches.

        Returns:
            JanitorResult with counts of cleaned items.
        """
        from kagan.core.adapters.db.schema import Repo

        async with get_session(self._session_factory) as session:
            result = await session.execute(select(Repo))
            repos = list(result.scalars().all())

        total_pruned = 0
        deleted_branches: list[str] = []
        processed_repos: list[str] = []

        for repo in repos:
            if not Path(repo.path).exists():
                continue

            processed_repos.append(repo.name)

            if prune_worktrees:
                pruned = await self._git.prune_worktrees(repo.path)
                total_pruned += pruned

            if gc_branches:
                branches = await self._git.list_kagan_branches(repo.path)
                for branch in branches:
                    workspace_id = self._extract_workspace_id_from_branch(branch)
                    if workspace_id and workspace_id in valid_workspace_ids:
                        continue

                    worktree = await self._git.get_worktree_for_branch(repo.path, branch)
                    if worktree is not None:
                        continue

                    deleted = await self._git.delete_branch(repo.path, branch, force=False)
                    if deleted:
                        deleted_branches.append(f"{repo.name}:{branch}")

        return JanitorResult(
            worktrees_pruned=total_pruned,
            branches_deleted=deleted_branches,
            repos_processed=processed_repos,
        )

    def _extract_workspace_id_from_branch(self, branch_name: str) -> str | None:
        """Extract workspace ID from a kagan branch name.

        Branch naming conventions:
        - kagan/{workspace_id} -> workspace_id
        - kagan/merge-worktree-{repo_id} -> None (merge worktrees handled separately)

        Returns None if the branch doesn't match the expected pattern.
        """
        if not branch_name.startswith("kagan/"):
            return None

        suffix = branch_name[6:]

        if suffix.startswith("merge-worktree-"):
            return None

        return suffix if suffix else None

    # ------------------------------------------------------------------
    # Git operations
    # ------------------------------------------------------------------

    async def create(self, task_id: str, base_branch: str | None = None) -> Path:
        """Create a workspace for the task and return primary worktree path."""
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
                target_branch=base_branch or repo.default_branch or "main",
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

    # ------------------------------------------------------------------
    # Internal DB helpers
    # ------------------------------------------------------------------

    async def _get_workspace_repo_rows(self, workspace_id: str) -> list[tuple[WorkspaceRepo, Repo]]:
        from kagan.core.adapters.db.schema import Repo, WorkspaceRepo

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
        from kagan.core.adapters.db.schema import Workspace

        async with get_session(self._session_factory) as session:
            return await session.get(Workspace, workspace_id)

    async def _get_latest_workspace_for_task(self, task_id: str) -> DbWorkspace | None:
        from kagan.core.adapters.db.schema import Workspace

        async with get_session(self._session_factory) as session:
            result = await session.execute(
                select(Workspace)
                .where(Workspace.task_id == task_id)
                .order_by(col(Workspace.created_at).desc())
            )
            return result.scalars().first()

    async def _get_primary_workspace_repo(self, workspace_id: str) -> WorkspaceRepo | None:
        from kagan.core.adapters.db.schema import ProjectRepo, Workspace, WorkspaceRepo

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
