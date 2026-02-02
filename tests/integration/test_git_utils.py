"""Tests for git_utils module - all async functions."""

from __future__ import annotations

import asyncio
from collections.abc import Callable, Coroutine
from typing import TYPE_CHECKING, Any
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

if TYPE_CHECKING:
    from pathlib import Path

from kagan.git_utils import (
    GitVersion,
    get_current_branch,
    has_git_repo,
    init_git_repo,
    list_local_branches,
)
from tests.helpers.git import configure_git_user

pytestmark = pytest.mark.integration
SetupFn = Callable[["Path"], Coroutine[Any, Any, None]]


async def _git(repo: Path, *args: str) -> None:
    """Run git command silently."""
    proc = await asyncio.create_subprocess_exec(
        "git", *args, cwd=repo, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE
    )
    await proc.communicate()


async def _init_repo_with_commit(path: Path, files: dict[str, str] | None = None) -> None:
    """Initialize repo with optional files and commit."""
    await _git(path, "init", "-b", "main")
    await configure_git_user(path)
    for name, content in (files or {}).items():
        (path / name).write_text(content)
    if files:
        await _git(path, "add", ".")
        await _git(path, "commit", "-m", "Initial commit")


# Setup functions for parametrized scenario tests
async def _setup_empty(p: Path) -> None:
    await configure_git_user(p)


async def _setup_with_gitignore(p: Path) -> None:
    await _init_repo_with_commit(p, {".gitignore": "node_modules/\n__pycache__/\n"})


async def _setup_without_gitignore(p: Path) -> None:
    await _init_repo_with_commit(p, {"README.md": "# Project\n"})


async def _setup_kagan_in_gitignore(p: Path) -> None:
    await _init_repo_with_commit(p, {".gitignore": "node_modules/\n.kagan/\n"})


async def _setup_no_commits(p: Path) -> None:
    await _git(p, "init", "-b", "main")
    await configure_git_user(p)


class TestHasGitRepo:
    """Tests for has_git_repo function."""

    @pytest.mark.parametrize(
        ("setup", "expected"),
        [
            pytest.param(lambda p: _git(p, "init"), True, id="valid_repo"),
            pytest.param(lambda p: asyncio.sleep(0), False, id="non_repo"),
        ],
    )
    async def test_has_git_repo(self, tmp_path: Path, setup, expected: bool) -> None:
        await setup(tmp_path)
        assert await has_git_repo(tmp_path) is expected

    async def test_returns_false_when_git_not_found(self, tmp_path: Path) -> None:
        with patch("asyncio.create_subprocess_exec", side_effect=FileNotFoundError):
            assert await has_git_repo(tmp_path) is False


class TestListLocalBranches:
    """Tests for list_local_branches function."""

    async def test_returns_empty_for_non_repo(self, tmp_path: Path) -> None:
        assert await list_local_branches(tmp_path) == []

    async def test_returns_branches_for_valid_repo(self, tmp_path: Path) -> None:
        await _init_repo_with_commit(tmp_path, {"file.txt": "content"})
        assert "main" in await list_local_branches(tmp_path)


class TestGetCurrentBranch:
    """Tests for get_current_branch function."""

    async def test_returns_empty_for_non_repo(self, tmp_path: Path) -> None:
        assert await get_current_branch(tmp_path) == ""

    async def test_returns_branch_name(self, tmp_path: Path) -> None:
        await _init_repo_with_commit(tmp_path, {"file.txt": "content"})
        assert await get_current_branch(tmp_path) == "main"


class TestInitGitRepo:
    """Tests for init_git_repo function."""

    async def test_returns_error_when_git_not_found(self, tmp_path: Path) -> None:
        with patch("asyncio.create_subprocess_exec", side_effect=FileNotFoundError):
            result = await init_git_repo(tmp_path, "main")
            assert result.success is False
            assert result.error is not None and result.error.error_type == "version_low"

    async def test_fallback_for_old_git_versions(self, tmp_path: Path) -> None:
        """When git init -b fails, falls back to git init + git branch -M."""
        await configure_git_user(tmp_path)
        call_count, original = [0], asyncio.create_subprocess_exec

        async def mock_subprocess(*args, **kwargs):
            call_count[0] += 1
            if args[1] in ("--version", "config"):
                return await original(*args, **kwargs)
            if call_count[0] == 3:
                proc = MagicMock()
                proc.communicate = AsyncMock(return_value=(b"", b"error"))
                proc.returncode = 1
                return proc
            return await original(*args, **kwargs)

        with patch("asyncio.create_subprocess_exec", side_effect=mock_subprocess):
            assert (await init_git_repo(tmp_path, "main")).success is True

    async def test_returns_error_if_all_init_attempts_fail(self, tmp_path: Path) -> None:
        """When all init attempts fail, returns error result."""
        original = asyncio.create_subprocess_exec

        async def mock_subprocess(*args, **kwargs):
            if len(args) > 1 and args[1] in ("--version", "config"):
                return await original(*args, **kwargs)
            proc = MagicMock()
            proc.communicate = AsyncMock(return_value=(b"", b"error"))
            proc.returncode = 1
            return proc

        with patch("asyncio.create_subprocess_exec", side_effect=mock_subprocess):
            result = await init_git_repo(tmp_path, "main")
            assert result.success is False and result.error is not None


class TestInitGitRepoScenarios:
    """Tests for the gitignore scenarios in init_git_repo."""

    @pytest.mark.parametrize(
        ("setup_fn", "expect_created", "expect_updated", "expect_committed"),
        [
            pytest.param(_setup_empty, True, False, True, id="empty_folder_no_repo"),
            pytest.param(_setup_with_gitignore, False, True, True, id="existing_with_gitignore"),
            pytest.param(
                _setup_without_gitignore, True, False, True, id="existing_without_gitignore"
            ),
            pytest.param(
                _setup_kagan_in_gitignore, False, False, False, id="kagan_already_present"
            ),
            pytest.param(_setup_no_commits, True, False, True, id="repo_no_commits"),
        ],
    )
    async def test_gitignore_scenarios(
        self,
        tmp_path: Path,
        setup_fn: SetupFn,
        expect_created: bool,
        expect_updated: bool,
        expect_committed: bool,
    ) -> None:
        """Test init_git_repo with various initial states."""
        await setup_fn(tmp_path)
        result = await init_git_repo(tmp_path, "main")
        assert result.success is True
        assert result.gitignore_created is expect_created
        assert result.gitignore_updated is expect_updated
        assert result.committed is expect_committed
        if expect_created or expect_updated:
            assert ".kagan/" in (tmp_path / ".gitignore").read_text()


class TestInitGitRepoErrors:
    """Tests for git error handling in init_git_repo."""

    @pytest.mark.parametrize(
        ("mock_target", "mock_return", "error_type", "msg_check"),
        [
            (
                "kagan.git_utils.get_git_version",
                GitVersion(major=2, minor=4, patch=0, raw="git version 2.4.0"),
                "version_low",
                lambda m: "2.4" in m,
            ),
            (
                "kagan.git_utils.get_git_version",
                None,
                "version_low",
                lambda m: "not installed" in m.lower(),
            ),
            (
                "kagan.git_utils.check_git_user_configured",
                (False, "Git user.name is not configured"),
                "user_not_configured",
                lambda m: "user" in m.lower(),
            ),
        ],
        ids=["version_too_low", "git_not_installed", "user_not_configured"],
    )
    async def test_init_errors(
        self,
        tmp_path: Path,
        mock_target: str,
        mock_return: Any,
        error_type: str,
        msg_check: Callable[[str], bool],
    ) -> None:
        """Test error handling for version/install/config issues."""
        with patch(mock_target, return_value=mock_return):
            result = await init_git_repo(tmp_path, "main")
            assert result.success is False
            assert result.error is not None and result.error.error_type == error_type
            assert msg_check(result.error.message)

    async def test_commit_failure(self, tmp_path: Path) -> None:
        """When commit fails (not 'nothing to commit'), returns commit_failed error."""
        await configure_git_user(tmp_path)
        original = asyncio.create_subprocess_exec

        async def mock_subprocess(*args, **kwargs):
            if len(args) > 1 and args[1] in ("--version", "config", "init", "add"):
                return await original(*args, **kwargs)
            if len(args) > 1 and args[1] == "commit":
                proc = MagicMock()
                proc.communicate = AsyncMock(return_value=(b"", b"fatal: email"))
                proc.returncode = 1
                return proc
            return await original(*args, **kwargs)

        with patch("asyncio.create_subprocess_exec", side_effect=mock_subprocess):
            result = await init_git_repo(tmp_path, "main")
            assert result.success is False
            assert result.error is not None and result.error.error_type == "commit_failed"
