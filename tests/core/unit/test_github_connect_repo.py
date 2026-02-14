"""Tests for GitHub plugin connect_repo operation and preflight checks."""

from __future__ import annotations

from typing import Any
from unittest.mock import MagicMock, patch

import pytest

from kagan.core.plugins.github import (
    GITHUB_CAPABILITY,
    GITHUB_METHOD_CONNECT_REPO,
    register_github_plugin,
)
from kagan.core.plugins.github.gh_adapter import (
    ALREADY_CONNECTED,
    GH_AUTH_REQUIRED,
    GH_CLI_NOT_AVAILABLE,
    GH_PROJECT_REQUIRED,
    GH_REPO_ACCESS_DENIED,
    GH_REPO_METADATA_INVALID,
    GH_REPO_REQUIRED,
    GITHUB_CONNECTION_KEY,
    GhAuthStatus,
    GhCliAdapterInfo,
    GhRepoView,
    PreflightError,
    parse_gh_repo_view,
    resolve_gh_cli,
    run_preflight_checks,
)
from kagan.core.plugins.sdk import PluginRegistry


class TestGhAdapter:
    """Tests for gh_adapter preflight check functions."""

    def test_resolve_gh_cli_returns_unavailable_when_gh_not_in_path(self) -> None:
        with patch("shutil.which", return_value=None):
            result = resolve_gh_cli()

        assert result.available is False
        assert result.path is None

    def test_resolve_gh_cli_returns_available_when_gh_in_path(self) -> None:
        with (
            patch("shutil.which", return_value="/usr/local/bin/gh"),
            patch("kagan.core.plugins.github.gh_adapter.run_exec_capture_sync") as mock_run_exec,
        ):
            mock_result = MagicMock()
            mock_result.returncode = 0
            mock_result.stdout_text.return_value = "gh version 2.40.0\n"
            mock_result.stderr_text.return_value = ""
            mock_run_exec.return_value = mock_result
            result = resolve_gh_cli()

        assert result.available is True
        assert result.path == "/usr/local/bin/gh"

    def test_parse_gh_repo_view_extracts_metadata_from_valid_json(self) -> None:
        raw = {
            "name": "kagan",
            "owner": {"login": "anthropics"},
            "url": "https://github.com/anthropics/kagan",
            "visibility": "PUBLIC",
            "defaultBranchRef": {"name": "main"},
            "sshUrl": "git@github.com:anthropics/kagan.git",
            "isPrivate": False,
        }

        result = parse_gh_repo_view(raw)

        assert isinstance(result, GhRepoView)
        assert result.host == "github.com"
        assert result.owner == "anthropics"
        assert result.name == "kagan"
        assert result.full_name == "anthropics/kagan"
        assert result.visibility == "PUBLIC"
        assert result.default_branch == "main"

    def test_parse_gh_repo_view_returns_error_when_owner_missing(self) -> None:
        raw = {"name": "kagan", "owner": {}}

        result = parse_gh_repo_view(raw)

        assert isinstance(result, PreflightError)
        assert result.code == GH_REPO_METADATA_INVALID

    def test_parse_gh_repo_view_defaults_branch_to_main_when_missing(self) -> None:
        raw = {
            "name": "kagan",
            "owner": {"login": "anthropics"},
            "url": "https://github.com/anthropics/kagan",
            "visibility": "PRIVATE",
        }

        result = parse_gh_repo_view(raw)

        assert isinstance(result, GhRepoView)
        assert result.default_branch == "main"

    def test_run_preflight_checks_returns_error_when_gh_not_available(self) -> None:
        with patch(
            "kagan.core.plugins.github.gh_adapter.resolve_gh_cli",
            return_value=GhCliAdapterInfo(available=False, path=None, version=None),
        ):
            repo_view, error = run_preflight_checks("/tmp/repo")

        assert repo_view is None
        assert error is not None
        assert error.code == GH_CLI_NOT_AVAILABLE
        assert "brew install gh" in error.hint

    def test_run_preflight_checks_returns_error_when_auth_fails(self) -> None:
        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.resolve_gh_cli",
                return_value=GhCliAdapterInfo(
                    available=True, path="/usr/local/bin/gh", version="2.40.0"
                ),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_auth_status",
                return_value=GhAuthStatus(
                    authenticated=False, username=None, error="Not logged in"
                ),
            ),
        ):
            repo_view, error = run_preflight_checks("/tmp/repo")

        assert repo_view is None
        assert error is not None
        assert error.code == GH_AUTH_REQUIRED
        assert "gh auth login" in error.hint

    def test_run_preflight_checks_returns_error_when_repo_access_denied(self) -> None:
        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.resolve_gh_cli",
                return_value=GhCliAdapterInfo(
                    available=True, path="/usr/local/bin/gh", version="2.40.0"
                ),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_auth_status",
                return_value=GhAuthStatus(authenticated=True, username="user", error=None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_repo_view",
                return_value=(None, "repository not found"),
            ),
        ):
            repo_view, error = run_preflight_checks("/tmp/repo")

        assert repo_view is None
        assert error is not None
        assert error.code == GH_REPO_ACCESS_DENIED

    def test_run_preflight_checks_returns_repo_view_on_success(self) -> None:
        mock_raw = {
            "name": "kagan",
            "owner": {"login": "anthropics"},
            "url": "https://github.com/anthropics/kagan",
            "visibility": "PUBLIC",
            "defaultBranchRef": {"name": "main"},
            "sshUrl": "git@github.com:anthropics/kagan.git",
        }
        with (
            patch(
                "kagan.core.plugins.github.gh_adapter.resolve_gh_cli",
                return_value=GhCliAdapterInfo(
                    available=True, path="/usr/local/bin/gh", version="2.40.0"
                ),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_auth_status",
                return_value=GhAuthStatus(authenticated=True, username="user", error=None),
            ),
            patch(
                "kagan.core.plugins.github.gh_adapter.run_gh_repo_view",
                return_value=(mock_raw, None),
            ),
        ):
            repo_view, error = run_preflight_checks("/tmp/repo")

        assert error is None
        assert repo_view is not None
        assert repo_view.full_name == "anthropics/kagan"


class TestConnectRepoRegistration:
    """Tests for connect_repo operation registration."""

    def test_connect_repo_operation_is_registered(self) -> None:
        registry = PluginRegistry()
        register_github_plugin(registry)

        operation = registry.resolve_operation(GITHUB_CAPABILITY, GITHUB_METHOD_CONNECT_REPO)

        assert operation is not None
        assert operation.mutating is True

    def test_connect_repo_operation_requires_maintainer_profile(self) -> None:
        from kagan.core.security import CapabilityProfile

        registry = PluginRegistry()
        register_github_plugin(registry)

        operation = registry.resolve_operation(GITHUB_CAPABILITY, GITHUB_METHOD_CONNECT_REPO)

        assert operation is not None
        assert operation.minimum_profile == CapabilityProfile.MAINTAINER


class TestConnectRepoHandler:
    """Tests for connect_repo handler logic."""

    @pytest.mark.asyncio()
    async def test_connect_repo_returns_error_when_project_id_missing(self) -> None:
        from kagan.core.plugins.github.runtime import handle_connect_repo

        ctx = MagicMock()
        params: dict[str, Any] = {}

        result = await handle_connect_repo(ctx, params)

        assert result["success"] is False
        assert result["code"] == GH_PROJECT_REQUIRED

    @pytest.mark.asyncio()
    async def test_connect_repo_returns_error_when_project_not_found(self) -> None:
        from kagan.core.plugins.github.runtime import handle_connect_repo

        ctx = MagicMock()
        ctx.project_service.get_project = MagicMock(return_value=None)

        # Make it async
        async def get_project_async(project_id: str) -> None:
            return None

        ctx.project_service.get_project = get_project_async
        params = {"project_id": "nonexistent"}

        result = await handle_connect_repo(ctx, params)

        assert result["success"] is False
        assert result["code"] == GH_PROJECT_REQUIRED

    @pytest.mark.asyncio()
    async def test_connect_repo_returns_error_when_project_has_no_repos(self) -> None:
        from kagan.core.plugins.github.runtime import handle_connect_repo

        ctx = MagicMock()

        async def get_project_async(project_id: str) -> MagicMock:
            return MagicMock(id=project_id)

        async def get_repos_async(project_id: str) -> list:
            return []

        ctx.project_service.get_project = get_project_async
        ctx.project_service.get_project_repos = get_repos_async
        params = {"project_id": "project-1"}

        result = await handle_connect_repo(ctx, params)

        assert result["success"] is False
        assert result["code"] == GH_REPO_REQUIRED

    @pytest.mark.asyncio()
    async def test_connect_repo_requires_repo_id_for_multi_repo_project(self) -> None:
        from kagan.core.plugins.github.runtime import handle_connect_repo

        ctx = MagicMock()

        async def get_project_async(project_id: str) -> MagicMock:
            return MagicMock(id=project_id)

        async def get_repos_async(project_id: str) -> list:
            return [
                MagicMock(id="repo-1", path="/tmp/repo1"),
                MagicMock(id="repo-2", path="/tmp/repo2"),
            ]

        ctx.project_service.get_project = get_project_async
        ctx.project_service.get_project_repos = get_repos_async
        params = {"project_id": "project-1"}

        result = await handle_connect_repo(ctx, params)

        assert result["success"] is False
        assert result["code"] == GH_REPO_REQUIRED
        assert "multi-repo" in result["message"]

    @pytest.mark.asyncio()
    async def test_connect_repo_returns_already_connected_when_metadata_exists(self) -> None:
        from kagan.core.plugins.github.runtime import handle_connect_repo

        ctx = MagicMock()
        existing_connection = {"host": "github.com", "owner": "test", "repo": "repo"}

        async def get_project_async(project_id: str) -> MagicMock:
            return MagicMock(id=project_id)

        async def get_repos_async(project_id: str) -> list:
            repo = MagicMock()
            repo.id = "repo-1"
            repo.path = "/tmp/repo"
            repo.scripts = {GITHUB_CONNECTION_KEY: existing_connection}
            return [repo]

        ctx.project_service.get_project = get_project_async
        ctx.project_service.get_project_repos = get_repos_async
        params = {"project_id": "project-1"}

        result = await handle_connect_repo(ctx, params)

        assert result["success"] is True
        assert result["code"] == ALREADY_CONNECTED

    @pytest.mark.asyncio()
    async def test_connect_repo_returns_error_when_preflight_fails(self) -> None:
        from kagan.core.plugins.github.runtime import handle_connect_repo

        ctx = MagicMock()

        async def get_project_async(project_id: str) -> MagicMock:
            return MagicMock(id=project_id)

        async def get_repos_async(project_id: str) -> list:
            repo = MagicMock()
            repo.id = "repo-1"
            repo.path = "/tmp/repo"
            repo.scripts = {}
            return [repo]

        ctx.project_service.get_project = get_project_async
        ctx.project_service.get_project_repos = get_repos_async
        params = {"project_id": "project-1"}

        with patch(
            "kagan.core.plugins.github.runtime.run_preflight_checks",
            return_value=(
                None,
                PreflightError(
                    code=GH_CLI_NOT_AVAILABLE,
                    message="gh not found",
                    hint="Install gh",
                ),
            ),
        ):
            result = await handle_connect_repo(ctx, params)

        assert result["success"] is False
        assert result["code"] == GH_CLI_NOT_AVAILABLE
        assert "hint" in result
