"""Diff service for per-repo operations."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Protocol

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession

    from kagan.adapters.db.repositories import ClosingAwareSessionFactory
    from kagan.adapters.git.operations import GitOperationsAdapter
    from kagan.services.workspaces import WorkspaceService


@dataclass
class FileDiff:
    """A single file's diff."""

    path: str
    additions: int
    deletions: int
    status: str
    diff_content: str


@dataclass
class RepoDiff:
    """Diff for a single repo."""

    repo_id: str
    repo_name: str
    target_branch: str
    files: list[FileDiff]
    total_additions: int
    total_deletions: int


class DiffService(Protocol):
    """Service for diff operations."""

    async def get_repo_diff(self, workspace_id: str, repo_id: str) -> RepoDiff:
        """Get diff for a single repo."""
        ...

    async def get_all_diffs(self, workspace_id: str) -> list[RepoDiff]:
        """Get diffs for all repos in a workspace."""
        ...

    async def get_unified_diff(self, workspace_id: str) -> str:
        """Get unified diff across all repos."""
        ...


class DiffServiceImpl:
    """Implementation of DiffService."""

    def __init__(
        self,
        session_factory: ClosingAwareSessionFactory,
        git_adapter: GitOperationsAdapter,
        workspace_service: WorkspaceService,
    ) -> None:
        self._session_factory = session_factory
        self._git = git_adapter
        self._workspace_service = workspace_service

    def _get_session(self) -> AsyncSession:
        """Get a new async session."""
        return self._session_factory()

    async def get_repo_diff(self, workspace_id: str, repo_id: str) -> RepoDiff:
        """Get diff for a single repo."""
        from sqlmodel import select

        from kagan.adapters.db.schema import Repo, WorkspaceRepo

        async with self._get_session() as session:
            result = await session.execute(
                select(WorkspaceRepo, Repo)
                .join(Repo)
                .where(WorkspaceRepo.workspace_id == workspace_id)
                .where(WorkspaceRepo.repo_id == repo_id)
            )
            row = result.first()

        if not row:
            raise ValueError(f"Repo {repo_id} not found in workspace {workspace_id}")

        workspace_repo, repo = row
        if not workspace_repo.worktree_path:
            raise ValueError(f"Repo {repo_id} has no worktree for workspace {workspace_id}")
        files = await self._git.get_file_diffs(
            workspace_repo.worktree_path,
            workspace_repo.target_branch,
        )

        return RepoDiff(
            repo_id=repo_id,
            repo_name=repo.name,
            target_branch=workspace_repo.target_branch,
            files=files,
            total_additions=sum(file.additions for file in files),
            total_deletions=sum(file.deletions for file in files),
        )

    async def get_all_diffs(self, workspace_id: str) -> list[RepoDiff]:
        """Get diffs for all repos in a workspace."""
        repos = await self._workspace_service.get_workspace_repos(workspace_id)
        diffs: list[RepoDiff] = []

        for repo in repos:
            diff = await self.get_repo_diff(workspace_id, repo["repo_id"])
            if diff.files or diff.total_additions or diff.total_deletions:
                diffs.append(diff)

        return diffs

    async def get_unified_diff(self, workspace_id: str) -> str:
        """Get unified diff across all repos for agent context."""
        diffs = await self.get_all_diffs(workspace_id)

        lines: list[str] = []
        for diff in diffs:
            lines.append(f"# === {diff.repo_name} ({diff.target_branch}) ===")
            lines.append(f"# +{diff.total_additions} -{diff.total_deletions}")
            lines.append("")
            for file in diff.files:
                lines.append(file.diff_content)
                lines.append("")

        return "\n".join(lines)
