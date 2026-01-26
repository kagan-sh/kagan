"""Tests for WorktreeManager."""

from __future__ import annotations

import tempfile
from pathlib import Path

import pytest

from kagan.agents.worktree import WorktreeError, WorktreeManager, slugify


class TestSlugify:
    """Tests for the slugify helper function."""

    def test_basic(self) -> None:
        assert slugify("Hello World") == "hello-world"

    def test_special_chars_and_numbers(self) -> None:
        assert slugify("Fix bug #123!") == "fix-bug-123"

    def test_truncation(self) -> None:
        result = slugify("This is a very long title that should be truncated", max_len=20)
        assert len(result) <= 20
        assert result == "this-is-a-very-long"

    def test_unicode(self) -> None:
        assert slugify("Café résumé") == "cafe-resume"

    def test_edge_cases(self) -> None:
        assert slugify("") == ""
        assert slugify("!@#$%^&*()") == ""
        assert slugify("--hello--world--") == "hello-world"


@pytest.fixture
async def git_repo():
    """Create a temporary git repository for testing."""
    with tempfile.TemporaryDirectory() as tmpdir:
        repo_path = Path(tmpdir)

        import asyncio

        # Initialize git repo with main branch
        proc = await asyncio.create_subprocess_exec(
            "git",
            "init",
            "-b",
            "main",
            cwd=repo_path,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        await proc.communicate()

        # Configure git user for commits
        proc = await asyncio.create_subprocess_exec(
            "git",
            "config",
            "user.email",
            "test@test.com",
            cwd=repo_path,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        await proc.communicate()

        proc = await asyncio.create_subprocess_exec(
            "git",
            "config",
            "user.name",
            "Test User",
            cwd=repo_path,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        await proc.communicate()

        # Disable GPG signing
        proc = await asyncio.create_subprocess_exec(
            "git",
            "config",
            "commit.gpgsign",
            "false",
            cwd=repo_path,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        await proc.communicate()

        # Create initial commit
        readme = repo_path / "README.md"
        readme.write_text("# Test Repo")

        proc = await asyncio.create_subprocess_exec(
            "git",
            "add",
            ".",
            cwd=repo_path,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        await proc.communicate()

        proc = await asyncio.create_subprocess_exec(
            "git",
            "commit",
            "-m",
            "Initial commit",
            cwd=repo_path,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        await proc.communicate()

        yield repo_path


class TestWorktreeManagerRepoRoot:
    """Tests for WorktreeManager repo_root parameter handling."""

    async def test_uses_provided_repo_root(self, git_repo: Path) -> None:
        """Test that WorktreeManager uses provided repo_root, not CWD."""
        import os

        # Change CWD to a different directory to ensure we're not using it
        original_cwd = os.getcwd()
        try:
            os.chdir(tempfile.gettempdir())

            # Create manager with explicit repo_root
            manager = WorktreeManager(repo_root=git_repo)

            # Verify repo_root is set correctly (not CWD)
            assert manager.repo_root == git_repo
            assert manager.repo_root != Path.cwd()
        finally:
            os.chdir(original_cwd)

    async def test_worktrees_dir_under_repo_root(self, git_repo: Path) -> None:
        """Test that worktrees_dir is derived from repo_root."""
        manager = WorktreeManager(repo_root=git_repo)

        expected_worktrees_dir = git_repo / ".kagan" / "worktrees"
        assert manager.worktrees_dir == expected_worktrees_dir

    async def test_defaults_to_cwd_when_not_provided(self) -> None:
        """Test that WorktreeManager defaults to CWD when repo_root not provided."""
        manager = WorktreeManager()

        assert manager.repo_root == Path.cwd()
        assert manager.worktrees_dir == Path.cwd() / ".kagan" / "worktrees"

    async def test_worktree_created_in_correct_location(self, git_repo: Path) -> None:
        """Test that worktrees are created under the specified repo_root."""
        import os

        # Change CWD to a different directory
        original_cwd = os.getcwd()
        try:
            os.chdir(tempfile.gettempdir())

            manager = WorktreeManager(repo_root=git_repo)
            path = await manager.create("test-ticket", "Test title")

            # Worktree should be under repo_root, not CWD
            assert path.is_relative_to(git_repo)
            assert not path.is_relative_to(Path.cwd())
            assert path == git_repo / ".kagan" / "worktrees" / "test-ticket"

            # Cleanup
            await manager.delete("test-ticket")
        finally:
            os.chdir(original_cwd)

    async def test_repo_root_with_nested_project(self) -> None:
        """Test repo_root handling simulating KaganApp's initialization pattern."""
        import asyncio

        with tempfile.TemporaryDirectory() as tmpdir:
            # Simulate project structure: /project/.kagan/config.toml
            project_root = Path(tmpdir) / "my_project"
            kagan_dir = project_root / ".kagan"
            config_path = kagan_dir / "config.toml"

            # Create directory structure
            kagan_dir.mkdir(parents=True)
            config_path.write_text("# config")

            # Initialize git repo in project root
            proc = await asyncio.create_subprocess_exec(
                "git",
                "init",
                "-b",
                "main",
                cwd=project_root,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            await proc.communicate()

            # Configure git
            for config_cmd in [
                ["config", "user.email", "test@test.com"],
                ["config", "user.name", "Test User"],
                ["config", "commit.gpgsign", "false"],
            ]:
                proc = await asyncio.create_subprocess_exec(
                    "git",
                    *config_cmd,
                    cwd=project_root,
                    stdout=asyncio.subprocess.PIPE,
                    stderr=asyncio.subprocess.PIPE,
                )
                await proc.communicate()

            # Create initial commit
            readme = project_root / "README.md"
            readme.write_text("# Test")

            proc = await asyncio.create_subprocess_exec(
                "git",
                "add",
                ".",
                cwd=project_root,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            await proc.communicate()

            proc = await asyncio.create_subprocess_exec(
                "git",
                "commit",
                "-m",
                "Initial commit",
                cwd=project_root,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            await proc.communicate()

            # Simulate KaganApp's initialization:
            # repo_root = config_path.parent.parent
            derived_repo_root = config_path.parent.parent
            assert derived_repo_root == project_root

            manager = WorktreeManager(repo_root=derived_repo_root)

            # Verify paths are correct
            assert manager.repo_root == project_root
            assert manager.worktrees_dir == project_root / ".kagan" / "worktrees"

            # Create a worktree and verify it's in the right place
            wt_path = await manager.create("ticket-abc", "Test ticket")
            assert wt_path == project_root / ".kagan" / "worktrees" / "ticket-abc"
            assert wt_path.exists()

            # Cleanup
            await manager.delete("ticket-abc")


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

        # Verify branch is deleted
        import asyncio

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
        # Should not raise
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
        # Branch should be kagan/ticket-001 (no slug)
        import asyncio

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
