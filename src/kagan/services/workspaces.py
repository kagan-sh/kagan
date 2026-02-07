"""Workspace service with multi-repo support."""

from __future__ import annotations

import asyncio
import contextlib
import shutil
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, Protocol

from sqlmodel import col, select

from kagan.core.events import WorkspaceProvisioned, WorkspaceReleased, WorkspaceRepoStatus
from kagan.core.models.entities import Workspace as DomainWorkspace
from kagan.core.models.enums import WorkspaceStatus
from kagan.paths import get_worktree_base_dir

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

    from kagan.adapters.db.schema import Repo, WorkspaceRepo
    from kagan.adapters.db.schema import Workspace as DbWorkspace
    from kagan.adapters.git.worktrees import GitWorktreeAdapter
    from kagan.core.models.entities import Workspace
    from kagan.services.projects import ProjectService
    from kagan.services.tasks import TaskService
    from kagan.services.types import RepoId, TaskId, WorkspaceId


@dataclass
class RepoWorkspaceInput:
    """Input for creating a workspace repo."""

    repo_id: str
    repo_path: str
    target_branch: str


class WorkspaceService(Protocol):
    """Service interface for workspace operations."""

    async def provision(
        self,
        task_id: TaskId,
        repos: list[RepoWorkspaceInput],
        *,
        branch_name: str | None = None,
    ) -> str:
        """Provision a new workspace with worktrees for all repos."""

    async def provision_for_project(
        self,
        task_id: TaskId,
        project_id: str,
        *,
        branch_name: str | None = None,
    ) -> str:
        """Provision a workspace using all repos from the project."""

    async def release(
        self,
        workspace_id: WorkspaceId,
        *,
        reason: str | None = None,
        cleanup: bool = True,
    ) -> None:
        """Release a workspace and optionally clean up worktrees."""

    async def get_workspace_repos(self, workspace_id: WorkspaceId) -> list[dict]:
        """Get all repos for a workspace with their worktree paths."""

    async def get_agent_working_dir(self, workspace_id: WorkspaceId) -> Path:
        """Get the working directory for agents (typically primary repo)."""

    async def get_workspace(self, workspace_id: WorkspaceId) -> Workspace | None:
        """Return a workspace by ID."""

    async def list_workspaces(
        self,
        *,
        task_id: TaskId | None = None,
        repo_id: RepoId | None = None,
    ) -> list[Workspace]:
        """List workspaces filtered by task or repo."""

    async def create(self, task_id: TaskId, base_branch: str = "main") -> Path:
        """Create a worktree for the task and return its path."""

    async def delete(self, task_id: TaskId, *, delete_branch: bool = False) -> None:
        """Delete a worktree and optionally its branch."""

    async def get_path(self, task_id: TaskId) -> Path | None:
        """Return the worktree path, if it exists."""

    async def get_commit_log(self, task_id: TaskId, base_branch: str = "main") -> list[str]:
        """Return commit messages for the task worktree."""

    async def get_diff(self, task_id: TaskId, base_branch: str = "main") -> str:
        """Return the diff between task branch and base."""

    async def get_diff_stats(self, task_id: TaskId, base_branch: str = "main") -> str:
        """Return diff stats for the task branch."""

    async def get_files_changed(self, task_id: TaskId, base_branch: str = "main") -> list[str]:
        """Return files changed by the task branch."""

    async def get_merge_worktree_path(self, task_id: TaskId, base_branch: str = "main") -> Path:
        """Return the merge worktree path, creating it if needed."""

    async def prepare_merge_conflicts(
        self, task_id: TaskId, base_branch: str = "main"
    ) -> tuple[bool, str]:
        """Prepare merge worktree for manual conflict resolution."""

    async def cleanup_orphans(self, valid_task_ids: set[TaskId]) -> list[str]:
        """Remove worktrees not associated with any known task."""

    async def rebase_onto_base(
        self, task_id: TaskId, base_branch: str = "main"
    ) -> tuple[bool, str, list[str]]:
        """Rebase the worktree branch onto the latest base branch."""

    async def get_files_changed_on_base(
        self, task_id: TaskId, base_branch: str = "main"
    ) -> list[str]:
        """Return files changed on the base branch since divergence."""


class WorkspaceServiceImpl:
    """Implementation of multi-repo WorkspaceService."""

    def __init__(
        self,
        session_factory: async_sessionmaker[AsyncSession],
        event_bus,
        git_adapter: GitWorktreeAdapter,
        task_service: TaskService,
        project_service: ProjectService,
    ) -> None:
        self._session_factory = session_factory
        self._events = event_bus
        self._git = git_adapter
        self._tasks = task_service
        self._projects = project_service
        self._merge_worktrees_dir = get_worktree_base_dir() / "merge-worktrees"

    def _get_session(self) -> AsyncSession:
        return self._session_factory()

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

            async with self._get_session() as session:
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

        await self._events.publish(
            WorkspaceProvisioned(
                workspace_id=workspace_id,
                task_id=task_id,
                branch=branch_name,
                path=str(base_dir),
                repo_count=len(repos),
            )
        )

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

        async with self._get_session() as session:
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

        async with self._get_session() as session:
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
            workspace.updated_at = datetime.now()
            session.add(workspace)
            await session.commit()

        await self._events.publish(
            WorkspaceReleased(
                workspace_id=workspace_id,
                task_id=workspace.task_id,
                reason=reason,
            )
        )

    async def get_workspace_repos(self, workspace_id: str) -> list[dict]:
        """Get all repos for a workspace with paths and status."""
        from kagan.adapters.db.schema import Repo, WorkspaceRepo

        async with self._get_session() as session:
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
            await self._events.publish(
                WorkspaceRepoStatus(
                    workspace_id=workspace_id,
                    repo_id=repo.id,
                    has_changes=has_changes,
                    diff_stats=diff_stats,
                )
            )

        return items

    async def get_agent_working_dir(self, workspace_id: str) -> Path:
        """Get working directory for agents (primary repo's worktree)."""
        primary_repo = await self._get_primary_workspace_repo(workspace_id)
        if primary_repo is None or not primary_repo.worktree_path:
            raise ValueError(f"Workspace {workspace_id} has no repos")
        return Path(primary_repo.worktree_path)

    async def get_workspace(self, workspace_id: str) -> Workspace | None:
        workspace = await self._get_workspace(workspace_id)
        if workspace is None:
            return None
        return DomainWorkspace.model_validate(workspace)

    async def list_workspaces(
        self,
        *,
        task_id: str | None = None,
        repo_id: str | None = None,
    ) -> list[Workspace]:
        from kagan.adapters.db.schema import Workspace, WorkspaceRepo

        async with self._get_session() as session:
            statement = select(Workspace).order_by(col(Workspace.created_at).desc())
            if task_id is not None:
                statement = statement.where(Workspace.task_id == task_id)
            if repo_id is not None:
                statement = (
                    statement.join(WorkspaceRepo).where(WorkspaceRepo.repo_id == repo_id).distinct()
                )
            result = await session.execute(statement)
            return [DomainWorkspace.model_validate(item) for item in result.scalars().all()]

    async def create(self, task_id: str, base_branch: str = "main") -> Path:
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

    async def get_merge_worktree_path(self, task_id: str, base_branch: str = "main") -> Path:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            raise ValueError(f"Workspace not found for task {task_id}")
        primary_repo = await self._get_primary_workspace_repo(workspace.id)
        if primary_repo is None:
            raise ValueError(f"Workspace {workspace.id} has no repos")
        return await self._ensure_merge_worktree(primary_repo.repo_id, base_branch, workspace)

    async def prepare_merge_conflicts(
        self, task_id: str, base_branch: str = "main"
    ) -> tuple[bool, str]:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            return False, f"Workspace not found for task {task_id}"
        branch_name = workspace.branch_name

        primary_repo = await self._get_primary_workspace_repo(workspace.id)
        if primary_repo is None:
            return False, f"Workspace {workspace.id} has no repos"
        merge_path = await self._ensure_merge_worktree(primary_repo.repo_id, base_branch, workspace)
        if await self._merge_in_progress(merge_path):
            return True, "Merge already in progress"

        try:
            await self._reset_merge_worktree(merge_path, base_branch)
            await self._run_git(
                "merge",
                "--squash",
                branch_name,
                cwd=merge_path,
                check=False,
            )
            status_out, _ = await self._run_git("status", "--porcelain", cwd=merge_path)
            if any(marker in status_out for marker in ("UU ", "AA ", "DD ")):
                return True, "Merge conflicts prepared"

            await self._run_git("merge", "--abort", cwd=merge_path, check=False)
            return False, "No conflicts detected"
        except Exception as exc:
            return False, f"Prepare failed: {exc}"

    async def cleanup_orphans(self, valid_task_ids: set[str]) -> list[str]:
        from kagan.adapters.db.schema import Workspace

        async with self._get_session() as session:
            result = await session.execute(select(Workspace))
            workspaces = result.scalars().all()

        cleaned: list[str] = []
        for workspace in workspaces:
            if workspace.task_id and workspace.task_id not in valid_task_ids:
                await self.release(workspace.id, cleanup=True)
                cleaned.append(workspace.id)

        return cleaned

    async def rebase_onto_base(
        self, task_id: str, base_branch: str = "main"
    ) -> tuple[bool, str, list[str]]:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            return False, f"Workspace not found for task {task_id}", []

        repo_rows = await self._get_workspace_repo_rows(workspace.id)
        if not repo_rows:
            return False, f"Workspace {workspace.id} has no repos", []

        try:
            for workspace_repo, repo in repo_rows:
                if not workspace_repo.worktree_path:
                    continue
                target_branch = workspace_repo.target_branch or base_branch
                wt_path = Path(workspace_repo.worktree_path)
                await self._run_git("fetch", "origin", target_branch, cwd=wt_path, check=False)

                status_out, _ = await self._run_git("status", "--porcelain", cwd=wt_path)
                if status_out.strip():
                    return (
                        False,
                        f"Cannot rebase: {repo.name} has uncommitted changes",
                        [],
                    )

                stdout, stderr = await self._run_git(
                    "rebase",
                    f"origin/{target_branch}",
                    cwd=wt_path,
                    check=False,
                )
                combined_output = f"{stdout}\n{stderr}".lower()
                if "conflict" in combined_output or "could not apply" in combined_output:
                    status_out, _ = await self._run_git("status", "--porcelain", cwd=wt_path)
                    conflicting_files: list[str] = []
                    for line in status_out.split("\n"):
                        if (
                            line.startswith("UU ")
                            or line.startswith("AA ")
                            or line.startswith("DD ")
                        ):
                            conflicting_files.append(f"{repo.name}:{line[3:].strip()}")

                    await self._run_git("rebase", "--abort", cwd=wt_path, check=False)
                    return (
                        False,
                        f"Rebase conflict in {repo.name} ({len(conflicting_files)} file(s))",
                        conflicting_files,
                    )

            return True, f"Successfully rebased onto {base_branch}", []
        except Exception as exc:
            with contextlib.suppress(Exception):
                for workspace_repo, _repo in repo_rows:
                    if not workspace_repo.worktree_path:
                        continue
                    await self._run_git(
                        "rebase",
                        "--abort",
                        cwd=Path(workspace_repo.worktree_path),
                        check=False,
                    )
            return False, f"Rebase failed: {exc}", []

    async def get_files_changed_on_base(self, task_id: str, base_branch: str = "main") -> list[str]:
        workspace = await self._get_latest_workspace_for_task(task_id)
        if workspace is None:
            return []

        repo_rows = await self._get_workspace_repo_rows(workspace.id)
        try:
            files: list[str] = []
            for workspace_repo, repo in repo_rows:
                if not workspace_repo.worktree_path:
                    continue
                target_branch = workspace_repo.target_branch or base_branch
                wt_path = Path(workspace_repo.worktree_path)
                merge_base_out, _ = await self._run_git(
                    "merge-base",
                    "HEAD",
                    f"origin/{target_branch}",
                    cwd=wt_path,
                    check=False,
                )
                if not merge_base_out.strip():
                    continue

                merge_base = merge_base_out.strip()
                diff_out, _ = await self._run_git(
                    "diff",
                    "--name-only",
                    merge_base,
                    f"origin/{target_branch}",
                    cwd=wt_path,
                )
                if not diff_out.strip():
                    continue

                repo_files = [line.strip() for line in diff_out.split("\n") if line.strip()]
                files.extend([f"{repo.name}:{path}" for path in repo_files])

            return files
        except Exception:
            return []

    async def _get_workspace_repo_rows(self, workspace_id: str) -> list[tuple[WorkspaceRepo, Repo]]:
        from kagan.adapters.db.schema import Repo, WorkspaceRepo

        async with self._get_session() as session:
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

        async with self._get_session() as session:
            return await session.get(Workspace, workspace_id)

    async def _get_latest_workspace_for_task(self, task_id: str) -> DbWorkspace | None:
        from kagan.adapters.db.schema import Workspace

        async with self._get_session() as session:
            result = await session.execute(
                select(Workspace)
                .where(Workspace.task_id == task_id)
                .order_by(col(Workspace.created_at).desc())
            )
            return result.scalars().first()

    async def _get_primary_workspace_repo(self, workspace_id: str) -> WorkspaceRepo | None:
        from kagan.adapters.db.schema import ProjectRepo, Workspace, WorkspaceRepo

        async with self._get_session() as session:
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

    async def _ensure_merge_worktree(
        self, repo_id: str, base_branch: str, workspace: DbWorkspace
    ) -> Path:
        merge_path = self._merge_worktrees_dir / repo_id
        merge_path.parent.mkdir(parents=True, exist_ok=True)

        if merge_path.exists():
            return merge_path

        worktree_path = await self.get_agent_working_dir(workspace.id)
        repo_root = self._resolve_repo_root(worktree_path)
        await self._run_git(
            "worktree",
            "add",
            "-B",
            self._merge_branch_name(repo_id),
            str(merge_path),
            base_branch,
            cwd=repo_root,
        )
        return merge_path

    async def _reset_merge_worktree(self, merge_path: Path, base_branch: str) -> Path:
        await self._run_git("fetch", "origin", base_branch, cwd=merge_path, check=False)

        base_ref = base_branch
        if await self._ref_exists(f"refs/remotes/origin/{base_branch}", cwd=merge_path):
            base_ref = f"origin/{base_branch}"

        await self._run_git("checkout", self._merge_branch_name(merge_path.name), cwd=merge_path)
        await self._run_git("reset", "--hard", base_ref, cwd=merge_path)
        return merge_path

    async def _ref_exists(self, ref: str, cwd: Path) -> bool:
        stdout, _ = await self._run_git(
            "rev-parse",
            "--verify",
            "--quiet",
            ref,
            cwd=cwd,
            check=False,
        )
        return bool(stdout.strip())

    async def _merge_in_progress(self, cwd: Path) -> bool:
        stdout, _ = await self._run_git(
            "rev-parse",
            "-q",
            "--verify",
            "MERGE_HEAD",
            cwd=cwd,
            check=False,
        )
        return bool(stdout.strip())

    def _resolve_repo_root(self, worktree_path: Path) -> Path:
        git_file = worktree_path / ".git"
        if not git_file.exists():
            return worktree_path
        content = git_file.read_text().strip()
        if not content.startswith("gitdir:"):
            return worktree_path
        git_dir = content.split(":", 1)[1].strip()
        return Path(git_dir).parent.parent.parent

    async def _run_git(
        self, *args: str, check: bool = True, cwd: Path | None = None
    ) -> tuple[str, str]:
        proc = await asyncio.create_subprocess_exec(
            "git",
            *args,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=cwd,
        )
        stdout, stderr = await proc.communicate()
        stdout_str = stdout.decode().strip()
        stderr_str = stderr.decode().strip()

        if check and proc.returncode != 0:
            raise RuntimeError(stderr_str or f"git {args[0]} failed with code {proc.returncode}")

        return stdout_str, stderr_str

    def _merge_branch_name(self, repo_id: str) -> str:
        return f"kagan/merge-worktree-{repo_id[:8]}"
