"""Tests for WelcomeScreen."""

from __future__ import annotations

from pathlib import Path

import pytest

from kagan.ui.screens.welcome import DEFAULT_BASE_BRANCHES, WelcomeScreen

pytestmark = pytest.mark.e2e


def _create_welcome_screen(
    has_git_repo: bool = True, repo_root: Path | None = None
) -> WelcomeScreen:
    """Create WelcomeScreen instance without full init for unit testing."""
    screen = WelcomeScreen.__new__(WelcomeScreen)
    screen._has_git_repo = has_git_repo
    screen._repo_root = repo_root or Path.cwd()
    return screen


class TestBuildBranchOptions:
    """Tests for _build_branch_options method."""

    def test_default_branch_first(self):
        screen = _create_welcome_screen()
        result = screen._build_branch_options(["feature", "develop"], "main")
        assert result[0] == "main"

    def test_deduplicates_branches(self):
        screen = _create_welcome_screen()
        result = screen._build_branch_options(["main", "develop", "main"], "main")
        assert result.count("main") == 1

    def test_includes_default_base_branches(self):
        screen = _create_welcome_screen()
        result = screen._build_branch_options([], "main")
        for branch in DEFAULT_BASE_BRANCHES:
            assert branch in result

    def test_preserves_order_default_then_branches_then_defaults(self):
        screen = _create_welcome_screen()
        result = screen._build_branch_options(["feature", "bugfix"], "develop")
        assert result[0] == "develop"
        assert result[1] == "feature"
        assert result[2] == "bugfix"

    def test_empty_branches_list(self):
        screen = _create_welcome_screen()
        result = screen._build_branch_options([], "main")
        assert "main" in result
        assert len(result) >= len(DEFAULT_BASE_BRANCHES)


class TestGetDefaultBaseBranch:
    """Tests for _get_default_base_branch method."""

    def test_no_git_repo_returns_main(self):
        screen = _create_welcome_screen(has_git_repo=False)
        result = screen._get_default_base_branch([])
        assert result == "main"

    def test_no_git_repo_ignores_branches(self):
        screen = _create_welcome_screen(has_git_repo=False)
        result = screen._get_default_base_branch(["develop", "feature"])
        assert result == "main"

    def test_with_git_repo_prefers_default_candidates(self, tmp_path: Path, monkeypatch):
        # Mock get_current_branch to return None
        monkeypatch.setattr("kagan.ui.screens.welcome.get_current_branch", lambda _: None)
        screen = _create_welcome_screen(has_git_repo=True, repo_root=tmp_path)
        result = screen._get_default_base_branch(["develop", "feature"])
        assert result == "develop"

    def test_with_git_repo_falls_back_to_first_branch(self, tmp_path: Path, monkeypatch):
        monkeypatch.setattr("kagan.ui.screens.welcome.get_current_branch", lambda _: None)
        screen = _create_welcome_screen(has_git_repo=True, repo_root=tmp_path)
        result = screen._get_default_base_branch(["custom-branch", "another"])
        assert result == "custom-branch"

    def test_with_git_repo_uses_current_branch(self, tmp_path: Path, monkeypatch):
        monkeypatch.setattr("kagan.ui.screens.welcome.get_current_branch", lambda _: "feature-x")
        screen = _create_welcome_screen(has_git_repo=True, repo_root=tmp_path)
        result = screen._get_default_base_branch(["main", "develop"])
        assert result == "feature-x"


class TestEnsureGitignored:
    """Tests for _ensure_gitignored method."""

    def test_creates_gitignore_if_missing(self, tmp_path: Path, monkeypatch):
        monkeypatch.chdir(tmp_path)
        screen = _create_welcome_screen(has_git_repo=True, repo_root=tmp_path)

        result = screen._ensure_gitignored()

        assert result is True
        gitignore = tmp_path / ".gitignore"
        assert gitignore.exists()
        content = gitignore.read_text()
        assert ".kagan/" in content

    def test_appends_to_existing_gitignore(self, tmp_path: Path, monkeypatch):
        monkeypatch.chdir(tmp_path)
        gitignore = tmp_path / ".gitignore"
        gitignore.write_text("node_modules/\n")

        screen = _create_welcome_screen(has_git_repo=True, repo_root=tmp_path)
        result = screen._ensure_gitignored()

        assert result is True
        content = gitignore.read_text()
        assert "node_modules/" in content
        assert ".kagan/" in content

    def test_appends_newline_if_missing(self, tmp_path: Path, monkeypatch):
        monkeypatch.chdir(tmp_path)
        gitignore = tmp_path / ".gitignore"
        gitignore.write_text("node_modules/")  # No trailing newline

        screen = _create_welcome_screen(has_git_repo=True, repo_root=tmp_path)
        screen._ensure_gitignored()

        content = gitignore.read_text()
        assert "\n\n# Kagan local state\n.kagan/" in content

    def test_skips_if_already_ignored_with_slash(self, tmp_path: Path, monkeypatch):
        monkeypatch.chdir(tmp_path)
        gitignore = tmp_path / ".gitignore"
        gitignore.write_text(".kagan/\n")

        screen = _create_welcome_screen(has_git_repo=True, repo_root=tmp_path)
        result = screen._ensure_gitignored()

        assert result is False

    def test_skips_if_already_ignored_without_slash(self, tmp_path: Path, monkeypatch):
        monkeypatch.chdir(tmp_path)
        gitignore = tmp_path / ".gitignore"
        gitignore.write_text(".kagan\n")

        screen = _create_welcome_screen(has_git_repo=True, repo_root=tmp_path)
        result = screen._ensure_gitignored()

        assert result is False
