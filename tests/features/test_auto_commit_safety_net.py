"""Tests for auto-commit safety net on dirty worktrees.

When agents signal <complete/>, or when merge/rebase is triggered,
uncommitted changes must be auto-committed instead of causing hard failures.
"""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, cast
from unittest.mock import AsyncMock, MagicMock, patch

from tests.helpers.git import configure_git_user, init_git_repo_with_commit
from tests.helpers.mocks import create_mock_workspace_service, create_test_config

from kagan.adapters.db.repositories import ExecutionRepository
from kagan.adapters.git.operations import GitOperationsAdapter
from kagan.adapters.git.worktrees import GitWorktreeAdapter
from kagan.bootstrap import InMemoryEventBus
from kagan.core.models.enums import TaskStatus, TaskType
from kagan.services.automation import AutomationServiceImpl
from kagan.services.runtime import RuntimeServiceImpl
from kagan.services.tasks import TaskServiceImpl

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.adapters.db.repositories import TaskRepository
    from kagan.services.projects import ProjectService


async def _init_repo(tmp_path: Path, name: str = "wt") -> Path:
    """Create a directory and init a git repo with an initial commit."""
    repo_dir = tmp_path / name
    repo_dir.mkdir(parents=True, exist_ok=True)
    return await init_git_repo_with_commit(repo_dir)


async def _make_dirty(repo_path: Path) -> None:
    """Modify a tracked file to create uncommitted changes."""
    (repo_path / "README.md").write_text("modified content\n")


class TestAutoCommitOnComplete:
    """Agent completes work but leaves uncommitted changes — they get committed."""

    async def test_handle_complete_auto_commits_dirty_worktree(
        self, state_manager: TaskRepository, task_factory, tmp_path: Path
    ):
        """_handle_complete auto-commits when worktree has uncommitted changes."""
        worktree = await _init_repo(tmp_path)
        await _make_dirty(worktree)

        task = await state_manager.create(
            task_factory(
                title="Agent left mess",
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.AUTO,
            )
        )

        event_bus = InMemoryEventBus()
        task_service = TaskServiceImpl(state_manager, event_bus)
        ws = create_mock_workspace_service()
        ws.get_path = AsyncMock(return_value=worktree)
        config = create_test_config(auto_review=False)
        git_adapter = GitOperationsAdapter()
        execution_service = ExecutionRepository(state_manager.session_factory)
        runtime_service = RuntimeServiceImpl(
            project_service=cast("ProjectService", MagicMock()),
            session_factory=state_manager.session_factory,
            execution_service=execution_service,
        )

        svc = AutomationServiceImpl(
            task_service,
            ws,
            config,
            execution_service=execution_service,
            event_bus=event_bus,
            git_adapter=git_adapter,
            runtime_service=runtime_service,
        )

        await svc._engine._handle_complete(task)

        # Worktree is now clean
        assert not await git_adapter.has_uncommitted_changes(str(worktree))
        # Task moved to REVIEW
        fetched = await state_manager.get(task.id)
        assert fetched is not None
        assert fetched.status == TaskStatus.REVIEW

    async def test_handle_complete_proceeds_when_worktree_clean(
        self, state_manager: TaskRepository, task_factory, tmp_path: Path
    ):
        """_handle_complete works normally when worktree is already clean."""
        worktree = await _init_repo(tmp_path)

        task = await state_manager.create(
            task_factory(
                title="Clean agent",
                status=TaskStatus.IN_PROGRESS,
                task_type=TaskType.AUTO,
            )
        )

        event_bus = InMemoryEventBus()
        task_service = TaskServiceImpl(state_manager, event_bus)
        ws = create_mock_workspace_service()
        ws.get_path = AsyncMock(return_value=worktree)
        config = create_test_config(auto_review=False)
        git_adapter = GitOperationsAdapter()
        execution_service = ExecutionRepository(state_manager.session_factory)
        runtime_service = RuntimeServiceImpl(
            project_service=cast("ProjectService", MagicMock()),
            session_factory=state_manager.session_factory,
            execution_service=execution_service,
        )

        svc = AutomationServiceImpl(
            task_service,
            ws,
            config,
            execution_service=execution_service,
            event_bus=event_bus,
            git_adapter=git_adapter,
            runtime_service=runtime_service,
        )

        await svc._engine._handle_complete(task)

        fetched = await state_manager.get(task.id)
        assert fetched is not None
        assert fetched.status == TaskStatus.REVIEW


class TestAutoCommitOnMerge:
    """Merge proceeds after auto-committing dirty worktree."""

    async def test_merge_auto_commits_instead_of_failing(self, tmp_path: Path):
        """merge_repo auto-commits uncommitted changes instead of returning failure."""
        from kagan.adapters.db.schema import Repo, Workspace, WorkspaceRepo
        from kagan.services.merges import MergeServiceImpl, MergeStrategy

        worktree = await _init_repo(tmp_path)
        await _make_dirty(worktree)

        git_adapter = GitOperationsAdapter()
        assert await git_adapter.has_uncommitted_changes(str(worktree))

        # Build MergeService with mocked dependencies
        merge_svc = MergeServiceImpl(
            MagicMock(),  # task_service
            MagicMock(),  # workspace_service
            MagicMock(),  # session_service
            MagicMock(merge_lock=asyncio.Lock()),  # automation_service
            create_test_config(),
            None,  # session_factory
            InMemoryEventBus(),
            git_adapter,
        )

        # Mock the DB query that merge_repo does internally
        workspace = MagicMock(spec=Workspace)
        workspace.id = "ws-1"
        workspace.task_id = "task-abc12345"
        workspace.branch_name = "test-branch"

        repo = MagicMock(spec=Repo)
        repo.id = "repo-1"
        repo.name = "test-repo"
        repo.path = str(worktree)

        workspace_repo = MagicMock(spec=WorkspaceRepo)
        workspace_repo.worktree_path = str(worktree)
        workspace_repo.target_branch = "main"

        mock_result = MagicMock()
        mock_result.first.return_value = (workspace_repo, repo, workspace)

        mock_session = MagicMock()
        mock_session.execute = AsyncMock(return_value=mock_result)
        mock_session.commit = AsyncMock()
        mock_session.add = MagicMock()
        mock_session.__aenter__ = AsyncMock(return_value=mock_session)
        mock_session.__aexit__ = AsyncMock(return_value=False)

        merge_svc._session_factory = MagicMock(return_value=mock_session)

        # Mock push (no remote in test repo) and PR creation
        git_adapter.push = AsyncMock()
        merge_svc._create_pr = AsyncMock(return_value="https://github.com/test/pr/1")

        result = await merge_svc.merge_repo("ws-1", "repo-1", strategy=MergeStrategy.PULL_REQUEST)

        # Worktree was auto-committed (no longer dirty)
        assert not await git_adapter.has_uncommitted_changes(str(worktree))
        # Merge succeeded
        assert result.success


class TestAutoCommitOnRebase:
    """Rebase proceeds after auto-committing dirty worktree."""

    async def test_rebase_auto_commits_instead_of_failing(self, tmp_path: Path):
        """rebase_onto_base auto-commits uncommitted changes instead of returning failure."""
        from kagan.adapters.db.schema import Repo, Workspace, WorkspaceRepo
        from kagan.services.workspaces import WorkspaceServiceImpl

        # Set up bare repo + cloned worktree with a dirty file
        bare = tmp_path / "bare.git"
        bare.mkdir()
        proc = await asyncio.create_subprocess_exec(
            "git",
            "init",
            "--bare",
            "-b",
            "main",
            cwd=bare,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        await proc.communicate()

        worktree = tmp_path / "wt"
        proc = await asyncio.create_subprocess_exec(
            "git",
            "clone",
            str(bare),
            str(worktree),
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        await proc.communicate()
        await configure_git_user(worktree)

        # Create initial commit and push to bare, then create feature branch
        (worktree / "README.md").write_text("# Test\n")
        for cmd in [
            ("add", "."),
            ("commit", "-m", "Initial commit"),
            ("push", "origin", "main"),
            ("checkout", "-b", "feature"),
        ]:
            proc = await asyncio.create_subprocess_exec(
                "git",
                *cmd,
                cwd=worktree,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            await proc.communicate()

        # Make the worktree dirty (modify a tracked file)
        (worktree / "README.md").write_text("modified content\n")

        # Build mock workspace/repo objects
        workspace_mock = MagicMock(spec=Workspace)
        workspace_mock.id = "ws-1"
        workspace_mock.task_id = "task-1"
        workspace_mock.branch_name = "feature"

        repo_mock = MagicMock(spec=Repo)
        repo_mock.id = "repo-1"
        repo_mock.name = "test-repo"
        repo_mock.path = str(worktree)

        workspace_repo_mock = MagicMock(spec=WorkspaceRepo)
        workspace_repo_mock.repo_id = "repo-1"
        workspace_repo_mock.worktree_path = str(worktree)
        workspace_repo_mock.target_branch = "main"

        # Create WorkspaceService with __new__ then mock DB-access methods
        ws_svc = WorkspaceServiceImpl.__new__(WorkspaceServiceImpl)
        ws_svc._git = GitWorktreeAdapter()

        with (
            patch.object(
                ws_svc, "_get_latest_workspace_for_task", new_callable=AsyncMock
            ) as mock_get_ws,
            patch.object(
                ws_svc, "_get_workspace_repo_rows", new_callable=AsyncMock
            ) as mock_get_rows,
        ):
            mock_get_ws.return_value = workspace_mock
            mock_get_rows.return_value = [(workspace_repo_mock, repo_mock)]

            success, message, conflicts = await ws_svc.rebase_onto_base(
                "task-1", base_branch="main"
            )

        # Rebase succeeded (not failed with "has uncommitted changes")
        assert success, f"Rebase failed: {message}"
        assert conflicts == []

        # The dirty file was committed — worktree is clean
        proc = await asyncio.create_subprocess_exec(
            "git",
            "status",
            "--porcelain",
            cwd=worktree,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _ = await proc.communicate()
        assert stdout.decode().strip() == "", "Worktree should be clean after auto-commit"
