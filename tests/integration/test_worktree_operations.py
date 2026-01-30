"""Tests for WorktreeManager - CRUD operations and semantic commit generation."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

import pytest

if TYPE_CHECKING:
    from pathlib import Path

from kagan.agents.worktree import WorktreeError, WorktreeManager

pytestmark = pytest.mark.integration


class TestWorktreeManager:
    """Tests for WorktreeManager class."""

    async def test_create_worktree(self, git_repo: Path) -> None:
        """Test creating a worktree."""
        manager = WorktreeManager(git_repo)
        path = await manager.create("ticket-001", "Fix login bug")

        assert path.exists()
        assert path == git_repo / ".kagan" / "worktrees" / "ticket-001"
        assert (path / "README.md").exists()

    async def test_create_duplicate_raises(self, git_repo: Path) -> None:
        """Test that creating a duplicate worktree raises error."""
        manager = WorktreeManager(git_repo)
        await manager.create("ticket-001", "First ticket")

        with pytest.raises(WorktreeError, match="already exists"):
            await manager.create("ticket-001", "Duplicate ticket")

    async def test_delete_worktree(self, git_repo: Path) -> None:
        """Test deleting a worktree."""
        manager = WorktreeManager(git_repo)
        path = await manager.create("ticket-001", "Test ticket")
        assert path.exists()

        await manager.delete("ticket-001")
        assert not path.exists()

    async def test_delete_worktree_with_branch(self, git_repo: Path) -> None:
        """Test deleting a worktree with branch cleanup."""
        manager = WorktreeManager(git_repo)
        await manager.create("ticket-001", "Test ticket")
        await manager.delete("ticket-001", delete_branch=True)

        proc = await asyncio.create_subprocess_exec(
            "git",
            "branch",
            "--list",
            "kagan/ticket-001-test-ticket",
            cwd=git_repo,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _ = await proc.communicate()
        assert stdout.decode().strip() == ""

    async def test_delete_nonexistent_noop(self, git_repo: Path) -> None:
        """Test that deleting non-existent worktree is a no-op."""
        manager = WorktreeManager(git_repo)
        await manager.delete("nonexistent-ticket")

    async def test_get_path_exists(self, git_repo: Path) -> None:
        """Test getting path of existing worktree."""
        manager = WorktreeManager(git_repo)
        created_path = await manager.create("ticket-001", "Test")

        path = await manager.get_path("ticket-001")
        assert path == created_path

    async def test_get_path_missing(self, git_repo: Path) -> None:
        """Test getting path of non-existent worktree returns None."""
        manager = WorktreeManager(git_repo)
        path = await manager.get_path("nonexistent")
        assert path is None

    async def test_list_all(self, git_repo: Path) -> None:
        """Test listing all worktrees."""
        manager = WorktreeManager(git_repo)
        await manager.create("ticket-001", "First")
        await manager.create("ticket-002", "Second")

        result = await manager.list_all()
        assert sorted(result) == ["ticket-001", "ticket-002"]

    async def test_list_all_after_delete(self, git_repo: Path) -> None:
        """Test that deleted worktrees are not listed."""
        manager = WorktreeManager(git_repo)
        await manager.create("ticket-001", "First")
        await manager.create("ticket-002", "Second")
        await manager.delete("ticket-001")

        result = await manager.list_all()
        assert result == ["ticket-002"]

    async def test_create_with_empty_title(self, git_repo: Path) -> None:
        """Test creating worktree with empty title uses just ticket ID."""
        manager = WorktreeManager(git_repo)
        path = await manager.create("ticket-001", "")

        assert path.exists()

        proc = await asyncio.create_subprocess_exec(
            "git",
            "branch",
            "--list",
            "kagan/ticket-001",
            cwd=git_repo,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _ = await proc.communicate()
        assert "kagan/ticket-001" in stdout.decode()


class TestGenerateSemanticCommit:
    """Tests for generate_semantic_commit method."""

    @pytest.fixture
    def manager(self) -> WorktreeManager:
        """Create a minimal WorktreeManager for testing."""
        return WorktreeManager()

    async def test_infers_fix_type(self, manager: WorktreeManager) -> None:
        """Test fix type inference from title keywords."""
        for keyword in ("fix", "bug", "issue"):
            result = await manager.generate_semantic_commit("t1", f"{keyword} something", [])
            assert result.startswith("fix:") or result.startswith("fix(")

    async def test_infers_feat_type(self, manager: WorktreeManager) -> None:
        """Test feat type inference from title keywords."""
        for keyword in ("add", "create", "implement", "new"):
            result = await manager.generate_semantic_commit("t1", f"{keyword} feature", [])
            assert result.startswith("feat:") or result.startswith("feat(")

    async def test_infers_refactor_type(self, manager: WorktreeManager) -> None:
        """Test refactor type inference from title keywords."""
        for keyword in ("refactor", "clean", "improve"):
            result = await manager.generate_semantic_commit("t1", f"{keyword} code", [])
            assert result.startswith("refactor:") or result.startswith("refactor(")

    async def test_infers_docs_type(self, manager: WorktreeManager) -> None:
        """Test docs type inference from title keywords."""
        for keyword in ("doc", "readme"):
            result = await manager.generate_semantic_commit("t1", f"Update {keyword}", [])
            assert result.startswith("docs:") or result.startswith("docs(")

    async def test_infers_test_type(self, manager: WorktreeManager) -> None:
        """Test test type inference from title keyword."""
        result = await manager.generate_semantic_commit("t1", "test coverage", [])
        assert result.startswith("test:") or result.startswith("test(")

    async def test_default_type_is_chore(self, manager: WorktreeManager) -> None:
        """Test chore is default when no keywords match."""
        result = await manager.generate_semantic_commit("t1", "Update dependencies", [])
        assert result.startswith("chore:") or result.startswith("chore(")

    async def test_extracts_scope_from_title(self, manager: WorktreeManager) -> None:
        """Test scope extraction from title."""
        result = await manager.generate_semantic_commit("t1", "Fix database connection", [])
        assert result.startswith("fix(database):")

    async def test_uses_ticket_title_in_message(self, manager: WorktreeManager) -> None:
        """Test that ticket title appears in commit message."""
        title = "Fix login validation"
        result = await manager.generate_semantic_commit("t1", title, [])
        assert title in result
