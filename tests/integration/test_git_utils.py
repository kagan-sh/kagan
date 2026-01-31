"""Tests for git_utils module - all async functions."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

if TYPE_CHECKING:
    from pathlib import Path

from kagan.git_utils import get_current_branch, has_git_repo, init_git_repo, list_local_branches
from tests.helpers.git import configure_git_user

pytestmark = pytest.mark.integration


async def _run_git(repo_path: Path, *args: str) -> None:
    """Run git command silently."""
    proc = await asyncio.create_subprocess_exec(
        "git",
        *args,
        cwd=repo_path,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    await proc.communicate()


class TestHasGitRepo:
    """Tests for has_git_repo function."""

    async def test_returns_true_for_valid_repo(self, tmp_path: Path) -> None:
        await _run_git(tmp_path, "init")
        assert await has_git_repo(tmp_path) is True

    async def test_returns_false_for_non_repo(self, tmp_path: Path) -> None:
        assert await has_git_repo(tmp_path) is False

    async def test_returns_false_when_git_not_found(self, tmp_path: Path) -> None:
        with patch("asyncio.create_subprocess_exec", side_effect=FileNotFoundError):
            assert await has_git_repo(tmp_path) is False


class TestListLocalBranches:
    """Tests for list_local_branches function."""

    async def test_returns_empty_for_non_repo(self, tmp_path: Path) -> None:
        assert await list_local_branches(tmp_path) == []

    async def test_returns_branches_for_valid_repo(self, tmp_path: Path) -> None:
        await _run_git(tmp_path, "init", "-b", "main")
        await configure_git_user(tmp_path)
        (tmp_path / "file.txt").write_text("content")
        await _run_git(tmp_path, "add", ".")
        await _run_git(tmp_path, "commit", "-m", "init")
        branches = await list_local_branches(tmp_path)
        assert "main" in branches

    async def test_returns_empty_when_git_not_found_on_branch_list(self, tmp_path: Path) -> None:
        """When has_git_repo succeeds but branch listing fails with FileNotFoundError."""
        await _run_git(tmp_path, "init")
        call_count = [0]

        async def mock_subprocess(*args, **kwargs):
            call_count[0] += 1
            if call_count[0] == 1:
                # First call (has_git_repo) - succeed
                mock_proc = MagicMock()
                mock_proc.communicate = AsyncMock(return_value=(b"true\n", b""))
                mock_proc.returncode = 0
                return mock_proc
            # Second call (branch list) - fail
            raise FileNotFoundError

        with patch("asyncio.create_subprocess_exec", side_effect=mock_subprocess):
            assert await list_local_branches(tmp_path) == []


class TestGetCurrentBranch:
    """Tests for get_current_branch function."""

    async def test_returns_empty_for_non_repo(self, tmp_path: Path) -> None:
        assert await get_current_branch(tmp_path) == ""

    async def test_returns_branch_name(self, tmp_path: Path) -> None:
        await _run_git(tmp_path, "init", "-b", "main")
        await configure_git_user(tmp_path)
        (tmp_path / "file.txt").write_text("content")
        await _run_git(tmp_path, "add", ".")
        await _run_git(tmp_path, "commit", "-m", "init")
        assert await get_current_branch(tmp_path) == "main"

    async def test_returns_empty_for_detached_head(self, tmp_path: Path) -> None:
        """When HEAD is detached, returns empty string."""
        call_count = [0]

        async def mock_subprocess(*args, **kwargs):
            call_count[0] += 1
            mock_proc = MagicMock()
            mock_proc.communicate = AsyncMock()
            if call_count[0] == 1:
                # has_git_repo check
                mock_proc.communicate.return_value = (b"true\n", b"")
                mock_proc.returncode = 0
            else:
                # get current branch - detached HEAD
                mock_proc.communicate.return_value = (b"HEAD\n", b"")
                mock_proc.returncode = 0
            return mock_proc

        with patch("asyncio.create_subprocess_exec", side_effect=mock_subprocess):
            assert await get_current_branch(tmp_path) == ""


class TestInitGitRepo:
    """Tests for init_git_repo function."""

    async def test_init_creates_repo_with_branch(self, tmp_path: Path) -> None:
        # Configure git globally for the test
        await _run_git(tmp_path, "config", "--global", "user.email", "test@test.com")
        await _run_git(tmp_path, "config", "--global", "user.name", "Test")
        result = await init_git_repo(tmp_path, "develop")
        assert result.success is True
        assert await has_git_repo(tmp_path)
        assert (tmp_path / ".gitignore").exists()
        # Verify .kagan/ is in .gitignore
        content = (tmp_path / ".gitignore").read_text()
        assert ".kagan/" in content or ".kagan" in content.split("\n")

    async def test_returns_error_when_git_not_found(self, tmp_path: Path) -> None:
        with patch("asyncio.create_subprocess_exec", side_effect=FileNotFoundError):
            result = await init_git_repo(tmp_path, "main")
            assert result.success is False
            assert result.error is not None
            assert result.error.error_type == "version_low"

    async def test_fallback_for_old_git_versions(self, tmp_path: Path) -> None:
        """When git init -b fails, falls back to git init + git branch -M."""
        await configure_git_user(tmp_path)
        call_count = [0]
        original_exec = asyncio.create_subprocess_exec

        async def mock_subprocess(*args, **kwargs):
            nonlocal call_count
            call_count[0] += 1
            # Allow git --version and git config calls through
            if args[1] == "--version" or args[1] == "config":
                return await original_exec(*args, **kwargs)
            if call_count[0] == 3:  # First git init -b call (after version and user checks)
                # First init -b call fails
                mock_proc = MagicMock()
                mock_proc.communicate = AsyncMock(return_value=(b"", b"error"))
                mock_proc.returncode = 1
                return mock_proc
            # Subsequent calls use real implementation
            return await original_exec(*args, **kwargs)

        with patch("asyncio.create_subprocess_exec", side_effect=mock_subprocess):
            result = await init_git_repo(tmp_path, "main")
            assert result.success is True

    async def test_returns_error_if_all_init_attempts_fail(self, tmp_path: Path) -> None:
        """When all init attempts fail, returns error result."""
        original_exec = asyncio.create_subprocess_exec

        async def mock_subprocess(*args, **kwargs):
            # Allow git --version and git config calls through
            if len(args) > 1 and (args[1] == "--version" or args[1] == "config"):
                return await original_exec(*args, **kwargs)
            # All other git commands fail
            mock_proc = MagicMock()
            mock_proc.communicate = AsyncMock(return_value=(b"", b"error"))
            mock_proc.returncode = 1
            return mock_proc

        with patch("asyncio.create_subprocess_exec", side_effect=mock_subprocess):
            result = await init_git_repo(tmp_path, "main")
            assert result.success is False
            assert result.error is not None

    async def test_includes_gitignore_in_initial_commit(self, tmp_path: Path) -> None:
        """When .gitignore is created, it should be included in the initial commit."""
        await _run_git(tmp_path, "config", "--global", "user.email", "test@test.com")
        await _run_git(tmp_path, "config", "--global", "user.name", "Test")

        result = await init_git_repo(tmp_path, "main")
        assert result.success is True

        # Verify .gitignore is tracked in the commit
        proc = await asyncio.create_subprocess_exec(
            "git",
            "ls-files",
            cwd=tmp_path,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _ = await proc.communicate()
        tracked_files = stdout.decode().strip().split("\n")
        assert ".gitignore" in tracked_files


class TestInitGitRepoScenarios:
    """Tests for the three gitignore scenarios in init_git_repo."""

    async def test_scenario1_empty_folder_no_git_repo(self, tmp_path: Path) -> None:
        """Scenario 1: Empty folder, no git repo.

        Expected: .gitignore is created with kagan exclusion, repo initialized, committed.
        """
        await configure_git_user(tmp_path)

        result = await init_git_repo(tmp_path, "main")

        assert result.success is True
        assert result.gitignore_created is True
        assert result.gitignore_updated is False
        assert result.committed is True
        assert (tmp_path / ".gitignore").exists()
        content = (tmp_path / ".gitignore").read_text()
        assert ".kagan/" in content

    async def test_scenario2_existing_repo_with_gitignore(self, tmp_path: Path) -> None:
        """Scenario 2: Existing git repo with .gitignore.

        Expected: .gitignore updated by appending kagan, commit made.
        """
        # Set up existing repo with .gitignore
        await _run_git(tmp_path, "init", "-b", "main")
        await configure_git_user(tmp_path)
        (tmp_path / ".gitignore").write_text("node_modules/\n__pycache__/\n")
        await _run_git(tmp_path, "add", ".gitignore")
        await _run_git(tmp_path, "commit", "-m", "Initial with gitignore")

        result = await init_git_repo(tmp_path, "main")

        assert result.success is True
        assert result.gitignore_created is False
        assert result.gitignore_updated is True
        assert result.committed is True
        content = (tmp_path / ".gitignore").read_text()
        # Original content preserved
        assert "node_modules/" in content
        assert "__pycache__/" in content
        # Kagan added
        assert ".kagan/" in content

    async def test_scenario3_existing_repo_without_gitignore(self, tmp_path: Path) -> None:
        """Scenario 3: Existing git repo without .gitignore.

        Expected: .gitignore created with kagan exclusion, commit made.
        """
        # Set up existing repo without .gitignore
        await _run_git(tmp_path, "init", "-b", "main")
        await configure_git_user(tmp_path)
        (tmp_path / "README.md").write_text("# Project\n")
        await _run_git(tmp_path, "add", "README.md")
        await _run_git(tmp_path, "commit", "-m", "Initial commit")

        result = await init_git_repo(tmp_path, "main")

        assert result.success is True
        assert result.gitignore_created is True
        assert result.gitignore_updated is False
        assert result.committed is True
        assert (tmp_path / ".gitignore").exists()
        content = (tmp_path / ".gitignore").read_text()
        assert ".kagan/" in content

    async def test_existing_repo_with_kagan_already_in_gitignore(self, tmp_path: Path) -> None:
        """When .kagan/ is already in .gitignore, no changes should be made."""
        # Set up existing repo with .kagan/ already in .gitignore
        await _run_git(tmp_path, "init", "-b", "main")
        await configure_git_user(tmp_path)
        (tmp_path / ".gitignore").write_text("node_modules/\n.kagan/\n")
        await _run_git(tmp_path, "add", ".gitignore")
        await _run_git(tmp_path, "commit", "-m", "Initial with gitignore")

        result = await init_git_repo(tmp_path, "main")

        assert result.success is True
        assert result.gitignore_created is False
        assert result.gitignore_updated is False
        assert result.committed is False  # Nothing to commit

    async def test_existing_repo_with_no_commits(self, tmp_path: Path) -> None:
        """Scenario 4: Repo initialized but no commits yet - should create initial commit."""
        # Set up existing repo with no commits (user ran git init but never committed)
        await _run_git(tmp_path, "init", "-b", "main")
        await configure_git_user(tmp_path)

        result = await init_git_repo(tmp_path, "main")

        assert result.success is True
        assert result.gitignore_created is True  # Created .gitignore
        assert result.gitignore_updated is False
        assert result.committed is True  # Created initial commit
        assert (tmp_path / ".gitignore").exists()
        content = (tmp_path / ".gitignore").read_text()
        assert ".kagan/" in content

        # Verify there's now a commit
        proc = await asyncio.create_subprocess_exec(
            "git",
            "log",
            "--oneline",
            cwd=tmp_path,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, _ = await proc.communicate()
        assert proc.returncode == 0
        assert "Initial commit (kagan)" in stdout.decode()


class TestInitGitRepoErrors:
    """Tests for git error handling in init_git_repo."""

    async def test_git_version_too_low(self, tmp_path: Path) -> None:
        """When git version is below 2.5, returns version_low error."""
        from kagan.git_utils import GitVersion

        with patch("kagan.git_utils.get_git_version") as mock_version:
            mock_version.return_value = GitVersion(
                major=2, minor=4, patch=0, raw="git version 2.4.0"
            )
            result = await init_git_repo(tmp_path, "main")

            assert result.success is False
            assert result.error is not None
            assert result.error.error_type == "version_low"
            assert "2.4" in result.error.message
            assert result.error.details is not None
            assert "2.5" in result.error.details

    async def test_git_not_installed(self, tmp_path: Path) -> None:
        """When git is not installed, returns version_low error with install message."""
        with patch("kagan.git_utils.get_git_version") as mock_version:
            mock_version.return_value = None
            result = await init_git_repo(tmp_path, "main")

            assert result.success is False
            assert result.error is not None
            assert result.error.error_type == "version_low"
            assert "not installed" in result.error.message.lower()

    async def test_git_user_not_configured(self, tmp_path: Path) -> None:
        """When git user is not configured, returns user_not_configured error."""
        with patch("kagan.git_utils.check_git_user_configured") as mock_user:
            mock_user.return_value = (False, "Git user.name is not configured")
            result = await init_git_repo(tmp_path, "main")

            assert result.success is False
            assert result.error is not None
            assert result.error.error_type == "user_not_configured"
            assert "user" in result.error.message.lower()

    async def test_commit_failure(self, tmp_path: Path) -> None:
        """When commit fails (not 'nothing to commit'), returns commit_failed error."""
        await configure_git_user(tmp_path)
        original_exec = asyncio.create_subprocess_exec
        commit_call_count = [0]

        async def mock_subprocess(*args, **kwargs):
            # Allow version check, user config, init, add to succeed
            if len(args) > 1 and args[1] in ("--version", "config", "init", "add"):
                return await original_exec(*args, **kwargs)
            if len(args) > 1 and args[1] == "commit":
                commit_call_count[0] += 1
                # Make commit fail with a real error
                mock_proc = MagicMock()
                mock_proc.communicate = AsyncMock(
                    return_value=(b"", b"fatal: unable to auto-detect email")
                )
                mock_proc.returncode = 1
                return mock_proc
            return await original_exec(*args, **kwargs)

        with patch("asyncio.create_subprocess_exec", side_effect=mock_subprocess):
            result = await init_git_repo(tmp_path, "main")

            assert result.success is False
            assert result.error is not None
            assert result.error.error_type == "commit_failed"
