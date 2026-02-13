"""Tests for workspace service janitor operations."""

from __future__ import annotations

from types import SimpleNamespace
from unittest.mock import AsyncMock

from kagan.core.services.workspaces.service import JanitorResult, WorkspaceServiceImpl


def _make_service_with_repos(
    monkeypatch,
    repos: list[SimpleNamespace],
    *,
    kagan_branches: list[str] | None = None,
    pruned_count: int = 0,
    worktree_for_branch: dict[str, str | None] | None = None,
    branch_delete_success: bool = True,
) -> WorkspaceServiceImpl:
    """Create a WorkspaceServiceImpl with mocked dependencies."""
    task_service = SimpleNamespace(get_task=AsyncMock(return_value=None))
    project_service = SimpleNamespace(get_project_repos=AsyncMock(return_value=[]))

    git_adapter = SimpleNamespace(
        prune_worktrees=AsyncMock(return_value=pruned_count),
        list_kagan_branches=AsyncMock(return_value=kagan_branches or []),
        get_worktree_for_branch=AsyncMock(
            side_effect=lambda repo, branch: (worktree_for_branch or {}).get(branch)
        ),
        delete_branch=AsyncMock(return_value=branch_delete_success),
    )

    # Create a mock scalars result that returns repos directly (not a coroutine)
    class MockScalarsResult:
        def all(self):
            return repos

    class MockExecuteResult:
        def scalars(self):
            return MockScalarsResult()

    mock_session = SimpleNamespace(
        execute=AsyncMock(return_value=MockExecuteResult()),
    )

    session_factory = SimpleNamespace()

    service = WorkspaceServiceImpl(
        session_factory=session_factory,
        git_adapter=git_adapter,
        task_service=task_service,
        project_service=project_service,
    )

    # Patch the DB session context manager
    import kagan.core.services.workspaces.service as ws_module

    class MockContextManager:
        async def __aenter__(self):
            return mock_session

        async def __aexit__(self, *args):
            pass

    monkeypatch.setattr(ws_module, "get_session", lambda session_factory: MockContextManager())

    return service


class TestJanitorResult:
    """Tests for JanitorResult dataclass."""

    def test_total_cleaned_returns_sum(self) -> None:
        result = JanitorResult(
            worktrees_pruned=3,
            branches_deleted=["repo:kagan/a", "repo:kagan/b"],
            repos_processed=["repo"],
        )

        assert result.total_cleaned == 5

    def test_total_cleaned_with_zeros(self) -> None:
        result = JanitorResult(
            worktrees_pruned=0,
            branches_deleted=[],
            repos_processed=["repo"],
        )

        assert result.total_cleaned == 0


class TestExtractWorkspaceIdFromBranch:
    """Tests for _extract_workspace_id_from_branch method."""

    def test_extracts_workspace_id_from_standard_branch(self, monkeypatch) -> None:
        service = _make_service_with_repos(monkeypatch, [])

        result = service._extract_workspace_id_from_branch("kagan/abc123")

        assert result == "abc123"

    def test_returns_none_for_merge_worktree_branch(self, monkeypatch) -> None:
        service = _make_service_with_repos(monkeypatch, [])

        result = service._extract_workspace_id_from_branch("kagan/merge-worktree-xyz789")

        assert result is None

    def test_returns_none_for_non_kagan_branch(self, monkeypatch) -> None:
        service = _make_service_with_repos(monkeypatch, [])

        result = service._extract_workspace_id_from_branch("feature/something")

        assert result is None

    def test_returns_none_for_empty_suffix(self, monkeypatch) -> None:
        service = _make_service_with_repos(monkeypatch, [])

        result = service._extract_workspace_id_from_branch("kagan/")

        assert result is None

    def test_handles_longer_workspace_ids(self, monkeypatch) -> None:
        service = _make_service_with_repos(monkeypatch, [])

        result = service._extract_workspace_id_from_branch("kagan/aeec2402")

        assert result == "aeec2402"


class TestRunJanitor:
    """Tests for run_janitor method."""

    async def test_prunes_worktrees_on_all_repos(self, tmp_path, monkeypatch) -> None:
        repo1 = SimpleNamespace(name="repo1", path=str(tmp_path / "repo1"))
        repo2 = SimpleNamespace(name="repo2", path=str(tmp_path / "repo2"))
        (tmp_path / "repo1").mkdir()
        (tmp_path / "repo2").mkdir()

        service = _make_service_with_repos(monkeypatch, [repo1, repo2], pruned_count=1)

        result = await service.run_janitor(set(), prune_worktrees=True, gc_branches=False)

        assert result.worktrees_pruned == 2  # 1 per repo
        assert result.repos_processed == ["repo1", "repo2"]
        assert service._git.prune_worktrees.call_count == 2

    async def test_skips_nonexistent_repo_paths(self, tmp_path, monkeypatch) -> None:
        repo1 = SimpleNamespace(name="repo1", path=str(tmp_path / "exists"))
        repo2 = SimpleNamespace(name="repo2", path=str(tmp_path / "nonexistent"))
        (tmp_path / "exists").mkdir()
        # Don't create nonexistent

        service = _make_service_with_repos(monkeypatch, [repo1, repo2], pruned_count=1)

        result = await service.run_janitor(set(), prune_worktrees=True, gc_branches=False)

        assert result.repos_processed == ["repo1"]
        assert service._git.prune_worktrees.call_count == 1

    async def test_deletes_orphan_branches_not_in_valid_set(self, tmp_path, monkeypatch) -> None:
        repo = SimpleNamespace(name="repo", path=str(tmp_path / "repo"))
        (tmp_path / "repo").mkdir()

        service = _make_service_with_repos(
            monkeypatch,
            [repo],
            kagan_branches=["kagan/orphan1", "kagan/valid", "kagan/orphan2"],
            worktree_for_branch={},  # None for all branches
        )

        result = await service.run_janitor(
            {"valid"},  # only "valid" workspace is active
            prune_worktrees=False,
            gc_branches=True,
        )

        assert len(result.branches_deleted) == 2
        assert "repo:kagan/orphan1" in result.branches_deleted
        assert "repo:kagan/orphan2" in result.branches_deleted

    async def test_preserves_branches_in_valid_workspace_ids(self, tmp_path, monkeypatch) -> None:
        repo = SimpleNamespace(name="repo", path=str(tmp_path / "repo"))
        (tmp_path / "repo").mkdir()

        service = _make_service_with_repos(
            monkeypatch,
            [repo],
            kagan_branches=["kagan/active1", "kagan/active2"],
            worktree_for_branch={},
        )

        result = await service.run_janitor(
            {"active1", "active2"},
            prune_worktrees=False,
            gc_branches=True,
        )

        assert result.branches_deleted == []

    async def test_preserves_branches_with_active_worktrees(self, tmp_path, monkeypatch) -> None:
        repo = SimpleNamespace(name="repo", path=str(tmp_path / "repo"))
        (tmp_path / "repo").mkdir()

        service = _make_service_with_repos(
            monkeypatch,
            [repo],
            kagan_branches=["kagan/orphan"],
            worktree_for_branch={"kagan/orphan": "/tmp/worktree"},  # Has worktree
        )

        result = await service.run_janitor(
            set(),  # No valid workspaces
            prune_worktrees=False,
            gc_branches=True,
        )

        assert result.branches_deleted == []

    async def test_handles_merge_worktree_branches(self, tmp_path, monkeypatch) -> None:
        repo = SimpleNamespace(name="repo", path=str(tmp_path / "repo"))
        (tmp_path / "repo").mkdir()

        service = _make_service_with_repos(
            monkeypatch,
            [repo],
            kagan_branches=["kagan/merge-worktree-abc", "kagan/orphan"],
            worktree_for_branch={},
        )

        result = await service.run_janitor(
            set(),
            prune_worktrees=False,
            gc_branches=True,
        )

        # merge-worktree branches should also be cleaned up as orphans
        # since they have no valid workspace ID
        assert len(result.branches_deleted) == 2

    async def test_combines_prune_and_gc(self, tmp_path, monkeypatch) -> None:
        repo = SimpleNamespace(name="repo", path=str(tmp_path / "repo"))
        (tmp_path / "repo").mkdir()

        service = _make_service_with_repos(
            monkeypatch,
            [repo],
            kagan_branches=["kagan/orphan"],
            pruned_count=2,
            worktree_for_branch={},
        )

        result = await service.run_janitor(
            set(),
            prune_worktrees=True,
            gc_branches=True,
        )

        assert result.worktrees_pruned == 2
        assert len(result.branches_deleted) == 1
        assert result.total_cleaned == 3
