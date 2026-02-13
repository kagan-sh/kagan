"""Tests for git worktree janitor operations: prune and branch GC."""

from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.core.adapters.git.worktrees import GitWorktreeAdapter

if TYPE_CHECKING:
    from collections.abc import Sequence
    from pathlib import Path


def _make_adapter(
    responses: dict[tuple[tuple[str, ...], bool | None], tuple[str, str]],
) -> tuple[GitWorktreeAdapter, list[tuple[tuple[str, ...], bool]]]:
    """Create adapter with mock git responses."""
    adapter = GitWorktreeAdapter(base_ref_strategy="remote")
    calls: list[tuple[tuple[str, ...], bool]] = []

    async def fake_run_git(
        cwd: Path,
        args: Sequence[str],
        *,
        check: bool = True,
    ) -> tuple[str, str]:
        del cwd
        command = tuple(args)
        calls.append((command, check))
        if (command, check) in responses:
            return responses[(command, check)]
        if (command, None) in responses:
            return responses[(command, None)]
        if check:
            raise RuntimeError(f"Unexpected git command: {' '.join(args)}")
        return "", ""

    async def fake_run_git_result(
        cwd: Path,
        args: Sequence[str],
    ) -> tuple[int, str, str]:
        del cwd
        command = tuple(args)
        calls.append((command, False))
        if (command, False) in responses:
            stdout, stderr = responses[(command, False)]
            return 0, stdout, stderr
        if (command, None) in responses:
            stdout, stderr = responses[(command, None)]
            return 0, stdout, stderr
        return 1, "", "not found"

    adapter._run_git = fake_run_git  # type: ignore[method-assign]
    adapter._run_git_result = fake_run_git_result  # type: ignore[method-assign]
    return adapter, calls


class TestPruneWorktrees:
    """Tests for prune_worktrees method."""

    async def test_prune_worktrees_runs_git_worktree_prune(self, tmp_path: Path) -> None:
        adapter, calls = _make_adapter(
            responses={
                (("worktree", "prune", "--verbose"), False): ("", ""),
            }
        )

        result = await adapter.prune_worktrees(str(tmp_path))

        assert (("worktree", "prune", "--verbose"), False) in calls
        assert result == 0

    async def test_prune_worktrees_counts_removed_entries(self, tmp_path: Path) -> None:
        adapter, _ = _make_adapter(
            responses={
                (("worktree", "prune", "--verbose"), False): (
                    "Removing worktrees/abc: not a valid directory\n"
                    "Removing worktrees/def: gitdir file points to non-existent location\n",
                    "",
                ),
            }
        )

        result = await adapter.prune_worktrees(str(tmp_path))

        assert result == 2

    async def test_prune_worktrees_returns_zero_for_nonexistent_path(self) -> None:
        adapter, _ = _make_adapter(responses={})

        result = await adapter.prune_worktrees("/nonexistent/path")

        assert result == 0


class TestListKaganBranches:
    """Tests for list_kagan_branches method."""

    async def test_lists_all_kagan_branches(self, tmp_path: Path) -> None:
        adapter, calls = _make_adapter(
            responses={
                (("for-each-ref", "--format=%(refname:short)", "refs/heads/kagan/*"), False): (
                    "kagan/abc123\nkagan/def456\nkagan/merge-worktree-xyz\n",
                    "",
                ),
            }
        )

        result = await adapter.list_kagan_branches(str(tmp_path))

        assert (
            ("for-each-ref", "--format=%(refname:short)", "refs/heads/kagan/*"),
            False,
        ) in calls
        assert result == ["kagan/abc123", "kagan/def456", "kagan/merge-worktree-xyz"]

    async def test_returns_empty_list_for_no_branches(self, tmp_path: Path) -> None:
        adapter, _ = _make_adapter(
            responses={
                (("for-each-ref", "--format=%(refname:short)", "refs/heads/kagan/*"), False): (
                    "",
                    "",
                ),
            }
        )

        result = await adapter.list_kagan_branches(str(tmp_path))

        assert result == []

    async def test_returns_empty_list_for_nonexistent_path(self) -> None:
        adapter, _ = _make_adapter(responses={})

        result = await adapter.list_kagan_branches("/nonexistent/path")

        assert result == []


class TestDeleteBranch:
    """Tests for delete_branch method."""

    async def test_delete_branch_uses_d_flag_by_default(self, tmp_path: Path) -> None:
        adapter, calls = _make_adapter(
            responses={
                (("branch", "-d", "kagan/abc123"), False): ("Deleted branch kagan/abc123\n", ""),
            }
        )

        result = await adapter.delete_branch(str(tmp_path), "kagan/abc123")

        assert (("branch", "-d", "kagan/abc123"), False) in calls
        assert result is True

    async def test_delete_branch_uses_D_flag_when_force(self, tmp_path: Path) -> None:
        adapter, calls = _make_adapter(
            responses={
                (("branch", "-D", "kagan/abc123"), False): ("Deleted branch kagan/abc123\n", ""),
            }
        )

        result = await adapter.delete_branch(str(tmp_path), "kagan/abc123", force=True)

        assert (("branch", "-D", "kagan/abc123"), False) in calls
        assert result is True

    async def test_delete_branch_returns_false_on_failure(self, tmp_path: Path) -> None:
        adapter, _ = _make_adapter(
            responses={}  # No response means command fails
        )

        result = await adapter.delete_branch(str(tmp_path), "kagan/nonexistent")

        assert result is False

    async def test_delete_branch_returns_false_for_nonexistent_path(self) -> None:
        adapter, _ = _make_adapter(responses={})

        result = await adapter.delete_branch("/nonexistent/path", "kagan/abc123")

        assert result is False


class TestIsBranchMerged:
    """Tests for is_branch_merged method."""

    async def test_returns_true_when_no_unmerged_commits(self, tmp_path: Path) -> None:
        adapter, _ = _make_adapter(
            responses={
                (("rev-parse", "--verify", "--quiet", "refs/remotes/origin/main"), False): (
                    "abc123",
                    "",
                ),
                (("rev-list", "--count", "origin/main..kagan/abc123"), False): ("0\n", ""),
            }
        )

        result = await adapter.is_branch_merged(str(tmp_path), "kagan/abc123", "main")

        assert result is True

    async def test_returns_false_when_has_unmerged_commits(self, tmp_path: Path) -> None:
        adapter, _ = _make_adapter(
            responses={
                (("rev-parse", "--verify", "--quiet", "refs/remotes/origin/main"), False): (
                    "abc123",
                    "",
                ),
                (("rev-list", "--count", "origin/main..kagan/abc123"), False): ("3\n", ""),
            }
        )

        result = await adapter.is_branch_merged(str(tmp_path), "kagan/abc123", "main")

        assert result is False

    async def test_returns_false_for_nonexistent_path(self) -> None:
        adapter, _ = _make_adapter(responses={})

        result = await adapter.is_branch_merged("/nonexistent/path", "kagan/abc123", "main")

        assert result is False


class TestGetWorktreeForBranch:
    """Tests for get_worktree_for_branch method."""

    async def test_returns_worktree_path_when_branch_checked_out(self, tmp_path: Path) -> None:
        adapter, _ = _make_adapter(
            responses={
                (("worktree", "list", "--porcelain"), False): (
                    "worktree /repo\n"
                    "HEAD abc123\n"
                    "branch refs/heads/main\n"
                    "\n"
                    "worktree /tmp/kagan/worktrees/abc123/repo\n"
                    "HEAD def456\n"
                    "branch refs/heads/kagan/abc123\n",
                    "",
                ),
            }
        )

        result = await adapter.get_worktree_for_branch(str(tmp_path), "kagan/abc123")

        assert result == "/tmp/kagan/worktrees/abc123/repo"

    async def test_returns_none_when_branch_not_checked_out(self, tmp_path: Path) -> None:
        adapter, _ = _make_adapter(
            responses={
                (("worktree", "list", "--porcelain"), False): (
                    "worktree /repo\nHEAD abc123\nbranch refs/heads/main\n",
                    "",
                ),
            }
        )

        result = await adapter.get_worktree_for_branch(str(tmp_path), "kagan/nonexistent")

        assert result is None

    async def test_returns_none_for_nonexistent_path(self) -> None:
        adapter, _ = _make_adapter(responses={})

        result = await adapter.get_worktree_for_branch("/nonexistent/path", "kagan/abc123")

        assert result is None
