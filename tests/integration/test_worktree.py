"""Tests for WorktreeManager - slugify and repo_root handling."""

from __future__ import annotations

import asyncio
import os
import tempfile
from pathlib import Path

import pytest

from kagan.agents.worktree import WorktreeManager, slugify

pytestmark = pytest.mark.integration


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
        assert slugify("Cafe resume") == "cafe-resume"

    def test_edge_cases(self) -> None:
        assert slugify("") == ""
        assert slugify("!@#$%^&*()") == ""
        assert slugify("--hello--world--") == "hello-world"


class TestWorktreeManagerRepoRoot:
    """Tests for WorktreeManager repo_root parameter handling."""

    async def test_uses_provided_repo_root(self, git_repo: Path) -> None:
        """Test that WorktreeManager uses provided repo_root, not CWD."""
        original_cwd = os.getcwd()
        try:
            os.chdir(tempfile.gettempdir())
            manager = WorktreeManager(repo_root=git_repo)
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
        original_cwd = os.getcwd()
        with tempfile.TemporaryDirectory() as other_dir:
            try:
                os.chdir(other_dir)
                manager = WorktreeManager(repo_root=git_repo)
                path = await manager.create("test-ticket", "Test title")

                assert path.is_relative_to(git_repo)
                assert not path.is_relative_to(Path.cwd())
                assert path == git_repo / ".kagan" / "worktrees" / "test-ticket"

                await manager.delete("test-ticket")
            finally:
                os.chdir(original_cwd)

    async def test_repo_root_with_nested_project(self) -> None:
        """Test repo_root handling simulating KaganApp's initialization pattern."""
        with tempfile.TemporaryDirectory() as tmpdir:
            project_root = Path(tmpdir) / "my_project"
            kagan_dir = project_root / ".kagan"
            config_path = kagan_dir / "config.toml"

            kagan_dir.mkdir(parents=True)
            config_path.write_text("# config")

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

            derived_repo_root = config_path.parent.parent
            assert derived_repo_root == project_root

            manager = WorktreeManager(repo_root=derived_repo_root)
            assert manager.repo_root == project_root
            assert manager.worktrees_dir == project_root / ".kagan" / "worktrees"

            wt_path = await manager.create("ticket-abc", "Test ticket")
            assert wt_path == project_root / ".kagan" / "worktrees" / "ticket-abc"
            assert wt_path.exists()

            await manager.delete("ticket-abc")
