"""Unit tests for WelcomeScreen methods."""

from __future__ import annotations

from pathlib import Path

import pytest

from kagan.ui.screens.welcome import DEFAULT_BASE_BRANCHES, WelcomeScreen

pytestmark = pytest.mark.unit


def _create_welcome_screen(
    has_git_repo: bool = True, repo_root: Path | None = None
) -> WelcomeScreen:
    """Create WelcomeScreen instance without full init for unit testing."""
    screen = WelcomeScreen.__new__(WelcomeScreen)
    screen._has_git_repo = has_git_repo
    screen._repo_root = repo_root or Path.cwd()
    return screen


class TestBuildBranchOptions:
    """Tests for _build_branch_options method - pure logic, no I/O."""

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


class TestGetDefaultBaseBranchNoRepo:
    """Tests for _get_default_base_branch when no git repo - pure logic."""

    async def test_no_git_repo_returns_main(self):
        screen = _create_welcome_screen(has_git_repo=False)
        result = await screen._get_default_base_branch([])
        assert result == "main"

    async def test_no_git_repo_ignores_branches(self):
        screen = _create_welcome_screen(has_git_repo=False)
        result = await screen._get_default_base_branch(["develop", "feature"])
        assert result == "main"
