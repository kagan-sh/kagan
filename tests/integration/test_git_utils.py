"""Tests for git_utils module - all functions."""

from __future__ import annotations

import subprocess
from typing import TYPE_CHECKING

import pytest

if TYPE_CHECKING:
    from pathlib import Path

from kagan.git_utils import get_current_branch, has_git_repo, init_git_repo, list_local_branches

pytestmark = pytest.mark.integration


class TestHasGitRepo:
    """Tests for has_git_repo function."""

    def test_returns_true_for_valid_repo(self, tmp_path: Path) -> None:
        subprocess.run(["git", "init"], cwd=tmp_path, capture_output=True)
        assert has_git_repo(tmp_path) is True

    def test_returns_false_for_non_repo(self, tmp_path: Path) -> None:
        assert has_git_repo(tmp_path) is False

    def test_returns_false_when_git_not_found(self, tmp_path: Path, mocker) -> None:
        mocker.patch("kagan.git_utils.subprocess.run", side_effect=FileNotFoundError)
        assert has_git_repo(tmp_path) is False

    def test_returns_false_on_nonzero_returncode(self, tmp_path: Path, mocker) -> None:
        mock_result = mocker.MagicMock()
        mock_result.returncode = 1
        mock_result.stdout = ""
        mocker.patch("kagan.git_utils.subprocess.run", return_value=mock_result)
        assert has_git_repo(tmp_path) is False

    def test_returns_false_when_stdout_not_true(self, tmp_path: Path, mocker) -> None:
        mock_result = mocker.MagicMock()
        mock_result.returncode = 0
        mock_result.stdout = "false\n"
        mocker.patch("kagan.git_utils.subprocess.run", return_value=mock_result)
        assert has_git_repo(tmp_path) is False


class TestListLocalBranches:
    """Tests for list_local_branches function."""

    def test_returns_empty_for_non_repo(self, tmp_path: Path) -> None:
        assert list_local_branches(tmp_path) == []

    def test_returns_branches_for_valid_repo(self, tmp_path: Path) -> None:
        subprocess.run(["git", "init", "-b", "main"], cwd=tmp_path, capture_output=True)
        subprocess.run(
            ["git", "config", "user.email", "test@test.com"], cwd=tmp_path, capture_output=True
        )
        subprocess.run(["git", "config", "user.name", "Test"], cwd=tmp_path, capture_output=True)
        subprocess.run(
            ["git", "config", "commit.gpgsign", "false"], cwd=tmp_path, capture_output=True
        )
        (tmp_path / "file.txt").write_text("content")
        subprocess.run(["git", "add", "."], cwd=tmp_path, capture_output=True)
        subprocess.run(["git", "commit", "-m", "init"], cwd=tmp_path, capture_output=True)
        branches = list_local_branches(tmp_path)
        assert "main" in branches

    def test_returns_empty_when_git_not_found(self, tmp_path: Path, mocker) -> None:
        call_count = [0]
        original_run = subprocess.run

        def mock_run(*args, **kwargs):
            call_count[0] += 1
            if call_count[0] == 1:
                return original_run(*args, **kwargs)
            raise FileNotFoundError

        subprocess.run(["git", "init"], cwd=tmp_path, capture_output=True)
        mocker.patch("kagan.git_utils.subprocess.run", side_effect=mock_run)
        assert list_local_branches(tmp_path) == []

    def test_returns_empty_on_nonzero_returncode(self, tmp_path: Path, mocker) -> None:
        subprocess.run(["git", "init"], cwd=tmp_path, capture_output=True)

        call_count = [0]

        def mock_run(*args, **kwargs):
            call_count[0] += 1
            if call_count[0] == 1:
                result = mocker.MagicMock()
                result.returncode = 0
                result.stdout = "true\n"
                return result
            result = mocker.MagicMock()
            result.returncode = 1
            result.stdout = ""
            return result

        mocker.patch("kagan.git_utils.subprocess.run", side_effect=mock_run)
        assert list_local_branches(tmp_path) == []


class TestGetCurrentBranch:
    """Tests for get_current_branch function."""

    def test_returns_empty_for_non_repo(self, tmp_path: Path) -> None:
        assert get_current_branch(tmp_path) == ""

    def test_returns_branch_name(self, tmp_path: Path) -> None:
        subprocess.run(["git", "init", "-b", "main"], cwd=tmp_path, capture_output=True)
        subprocess.run(
            ["git", "config", "user.email", "test@test.com"], cwd=tmp_path, capture_output=True
        )
        subprocess.run(["git", "config", "user.name", "Test"], cwd=tmp_path, capture_output=True)
        subprocess.run(
            ["git", "config", "commit.gpgsign", "false"], cwd=tmp_path, capture_output=True
        )
        (tmp_path / "file.txt").write_text("content")
        subprocess.run(["git", "add", "."], cwd=tmp_path, capture_output=True)
        subprocess.run(["git", "commit", "-m", "init"], cwd=tmp_path, capture_output=True)
        assert get_current_branch(tmp_path) == "main"

    def test_returns_empty_when_git_not_found(self, tmp_path: Path, mocker) -> None:
        call_count = [0]

        def mock_run(*args, **kwargs):
            call_count[0] += 1
            if call_count[0] == 1:
                result = mocker.MagicMock()
                result.returncode = 0
                result.stdout = "true\n"
                return result
            raise FileNotFoundError

        mocker.patch("kagan.git_utils.subprocess.run", side_effect=mock_run)
        assert get_current_branch(tmp_path) == ""

    def test_returns_empty_on_nonzero_returncode(self, tmp_path: Path, mocker) -> None:
        call_count = [0]

        def mock_run(*args, **kwargs):
            call_count[0] += 1
            if call_count[0] == 1:
                result = mocker.MagicMock()
                result.returncode = 0
                result.stdout = "true\n"
                return result
            result = mocker.MagicMock()
            result.returncode = 1
            result.stdout = ""
            return result

        mocker.patch("kagan.git_utils.subprocess.run", side_effect=mock_run)
        assert get_current_branch(tmp_path) == ""

    def test_returns_empty_for_detached_head(self, tmp_path: Path, mocker) -> None:
        call_count = [0]

        def mock_run(*args, **kwargs):
            call_count[0] += 1
            if call_count[0] == 1:
                result = mocker.MagicMock()
                result.returncode = 0
                result.stdout = "true\n"
                return result
            result = mocker.MagicMock()
            result.returncode = 0
            result.stdout = "HEAD\n"
            return result

        mocker.patch("kagan.git_utils.subprocess.run", side_effect=mock_run)
        assert get_current_branch(tmp_path) == ""


class TestInitGitRepo:
    """Tests for init_git_repo function."""

    def test_init_creates_repo_with_branch(self, tmp_path: Path) -> None:
        subprocess.run(
            ["git", "config", "--global", "user.email", "test@test.com"], capture_output=True
        )
        subprocess.run(["git", "config", "--global", "user.name", "Test"], capture_output=True)
        result = init_git_repo(tmp_path, "develop")
        assert result is True
        assert has_git_repo(tmp_path)
        assert (tmp_path / ".gitkeep").exists()

    def test_returns_false_when_git_not_found(self, tmp_path: Path, mocker) -> None:
        mocker.patch("kagan.git_utils.subprocess.run", side_effect=FileNotFoundError)
        assert init_git_repo(tmp_path, "main") is False

    def test_fallback_for_old_git_versions(self, tmp_path: Path, mocker) -> None:
        call_count = [0]
        original_run = subprocess.run

        def mock_run(cmd, **kwargs):
            call_count[0] += 1
            if call_count[0] == 1:
                result = mocker.MagicMock()
                result.returncode = 1
                return result
            return original_run(cmd, **kwargs)

        mocker.patch("kagan.git_utils.subprocess.run", side_effect=mock_run)
        result = init_git_repo(tmp_path, "main")
        assert result is True

    def test_returns_false_if_fallback_init_fails(self, tmp_path: Path, mocker) -> None:
        def mock_run(cmd, **kwargs):
            result = mocker.MagicMock()
            result.returncode = 1
            return result

        mocker.patch("kagan.git_utils.subprocess.run", side_effect=mock_run)
        assert init_git_repo(tmp_path, "main") is False

    def test_returns_false_if_branch_rename_fails(self, tmp_path: Path, mocker) -> None:
        call_count = [0]
        original_run = subprocess.run

        def mock_run(cmd, **kwargs):
            call_count[0] += 1
            if call_count[0] == 1:
                result = mocker.MagicMock()
                result.returncode = 1
                return result
            if call_count[0] == 2:
                return original_run(cmd, **kwargs)
            if call_count[0] == 3:
                result = mocker.MagicMock()
                result.returncode = 1
                return result
            return original_run(cmd, **kwargs)

        mocker.patch("kagan.git_utils.subprocess.run", side_effect=mock_run)
        assert init_git_repo(tmp_path, "main") is False

    def test_returns_false_if_git_add_fails(self, tmp_path: Path, mocker) -> None:
        original_run = subprocess.run

        def mock_run(cmd, **kwargs):
            if "add" in cmd:
                result = mocker.MagicMock()
                result.returncode = 1
                return result
            return original_run(cmd, **kwargs)

        mocker.patch("kagan.git_utils.subprocess.run", side_effect=mock_run)
        assert init_git_repo(tmp_path, "main") is False

    def test_returns_false_if_commit_fails(self, tmp_path: Path, mocker) -> None:
        original_run = subprocess.run

        def mock_run(cmd, **kwargs):
            if "commit" in cmd:
                result = mocker.MagicMock()
                result.returncode = 1
                return result
            return original_run(cmd, **kwargs)

        mocker.patch("kagan.git_utils.subprocess.run", side_effect=mock_run)
        assert init_git_repo(tmp_path, "main") is False
