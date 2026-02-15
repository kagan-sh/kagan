"""High-signal integration tests for CoreHost API dispatch."""

from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING, cast
from unittest.mock import AsyncMock, MagicMock

import pytest
from _api_helpers import build_api

from kagan.core.host import CoreHost
from kagan.core.ipc.contracts import CoreRequest
from kagan.core.models.enums import TaskStatus
from kagan.core.plugins.github.contract import GITHUB_CAPABILITY, GITHUB_METHOD_SYNC_ISSUES
from kagan.core.plugins.sdk import PluginManifest, PluginOperation
from kagan.core.security import CapabilityProfile

if TYPE_CHECKING:
    from pathlib import Path

    from kagan.core.bootstrap import AppContext


async def _dispatch(host: CoreHost, request: CoreRequest):
    return await host.handle_request(request)


@pytest.fixture
async def handle_host(tmp_path: Path):
    """Build a CoreHost wired with a real API boundary and auth profiles."""
    repo, api, ctx = await build_api(tmp_path)
    ctx.api = api

    host = CoreHost()
    host._ctx = cast("AppContext", ctx)
    host.register_session("maintainer-session", "maintainer")
    host.register_session("viewer-session", "viewer")

    yield host, api

    await repo.close()


class TestApiDispatchIntegration:
    """Keep only behavior checks that validate CoreHost -> API wiring."""

    async def test_task_create_dispatches_to_real_api(self, handle_host: tuple) -> None:
        host, _api = handle_host

        response = await _dispatch(
            host,
            CoreRequest(
                session_id="maintainer-session",
                capability="tasks",
                method="create",
                params={"title": "From API"},
            ),
        )

        assert response.ok
        assert response.result is not None
        assert response.result["success"] is True
        assert response.result["title"] == "From API"

    async def test_task_search_dispatches_to_real_api(self, handle_host: tuple) -> None:
        host, api = handle_host
        task = await api.create_task("Searchable Task")

        response = await _dispatch(
            host,
            CoreRequest(
                session_id="maintainer-session",
                capability="tasks",
                method="search",
                params={"query": "Searchable"},
            ),
        )

        assert response.ok
        assert response.result is not None
        ids = {item["id"] for item in response.result["tasks"]}
        assert task.id in ids

    async def test_task_status_change_from_tasks_capability_is_visible_via_tui_api_call(
        self,
        handle_host: tuple,
    ) -> None:
        host, _api = handle_host
        host.register_session("maintainer-session-2", "maintainer")

        create_response = await _dispatch(
            host,
            CoreRequest(
                session_id="maintainer-session",
                capability="tasks",
                method="create",
                params={"title": "Cross-client visibility"},
            ),
        )
        assert create_response.ok
        assert create_response.result is not None
        task_id = str(create_response.result["task_id"])

        move_response = await _dispatch(
            host,
            CoreRequest(
                session_id="maintainer-session",
                capability="tasks",
                method="move",
                params={"task_id": task_id, "status": "IN_PROGRESS"},
            ),
        )
        assert move_response.ok
        assert move_response.result is not None
        assert move_response.result["new_status"] == "IN_PROGRESS"

        read_response = await _dispatch(
            host,
            CoreRequest(
                session_id="maintainer-session-2",
                capability="tui",
                method="api_call",
                params={
                    "method": "get_task",
                    "kwargs": {"task_id": task_id},
                },
            ),
        )

        assert read_response.ok
        assert read_response.result is not None
        assert read_response.result["success"] is True
        payload = read_response.result["value"]
        assert payload["id"] == task_id
        assert payload["status"] == "IN_PROGRESS"

    async def test_plugin_ui_invoke_mutation_visible_across_clients_via_subsequent_read(
        self,
        handle_host: tuple,
    ) -> None:
        host, _api = handle_host
        host.register_session("maintainer-session-2", "maintainer")

        create_response = await _dispatch(
            host,
            CoreRequest(
                session_id="maintainer-session",
                capability="tasks",
                method="create",
                params={"title": "Plugin UI mutation"},
            ),
        )
        assert create_response.ok
        assert create_response.result is not None
        task_id = str(create_response.result["task_id"])

        async def _describe(ctx, params):
            del ctx, params
            return {
                "schema_version": "1",
                "actions": [
                    {
                        "plugin_id": "uitest",
                        "action_id": "touch",
                        "surface": "kanban.task_actions",
                        "label": "Touch task",
                        "operation": {"capability": "uitest", "method": "touch_task"},
                    }
                ],
            }

        async def _touch(ctx, params):
            target_task_id = str(params.get("task_id", "")).strip()
            await ctx.task_service.move(target_task_id, TaskStatus.IN_PROGRESS)
            return {"success": True, "code": "OK", "message": "Task moved"}

        plugin_registry = host._ctx.plugin_registry

        class _UiTestPlugin:
            manifest = PluginManifest(
                id="uitest",
                name="UI Test Plugin",
                version="0.0.0",
                entrypoint="tests",
            )

            def register(self, api) -> None:
                api.register_operation(
                    PluginOperation(
                        plugin_id="uitest",
                        capability="uitest",
                        method="ui_describe",
                        handler=_describe,
                        minimum_profile=CapabilityProfile.MAINTAINER,
                        mutating=False,
                    )
                )
                api.register_operation(
                    PluginOperation(
                        plugin_id="uitest",
                        capability="uitest",
                        method="touch_task",
                        handler=_touch,
                        minimum_profile=CapabilityProfile.MAINTAINER,
                        mutating=True,
                    )
                )

        plugin_registry.register_plugin(_UiTestPlugin())
        host._ctx.config.ui.tui_plugin_ui_allowlist = ["uitest"]

        invoke_response = await _dispatch(
            host,
            CoreRequest(
                session_id="maintainer-session",
                capability="tui",
                method="api_call",
                params={
                    "method": "plugin_ui_invoke",
                    "kwargs": {
                        "project_id": host._ctx.active_project_id,
                        "plugin_id": "uitest",
                        "action_id": "touch",
                        "inputs": {"task_id": task_id},
                    },
                },
            ),
        )
        assert invoke_response.ok
        assert invoke_response.result is not None
        assert invoke_response.result["success"] is True
        assert invoke_response.result["value"]["ok"] is True

        read_response = await _dispatch(
            host,
            CoreRequest(
                session_id="maintainer-session-2",
                capability="tui",
                method="api_call",
                params={
                    "method": "get_task",
                    "kwargs": {"task_id": task_id},
                },
            ),
        )
        assert read_response.ok
        assert read_response.result is not None
        payload = read_response.result["value"]
        assert payload["id"] == task_id
        assert payload["status"] == "IN_PROGRESS"

    async def test_plugin_ui_invoke_enforces_plugin_policy_hook(
        self,
        handle_host: tuple,
    ) -> None:
        host, _api = handle_host

        async def _describe(ctx, params):
            del ctx, params
            return {
                "schema_version": "1",
                "actions": [
                    {
                        "plugin_id": "uitest2",
                        "action_id": "blocked",
                        "surface": "kanban.repo_actions",
                        "label": "Blocked",
                        "operation": {"capability": "uitest2", "method": "blocked_op"},
                    }
                ],
            }

        async def _blocked(ctx, params):
            del ctx, params
            return {"success": True, "code": "OK", "message": "Should not run"}

        plugin_registry = host._ctx.plugin_registry

        class _UiTestPlugin2:
            manifest = PluginManifest(
                id="uitest2",
                name="UI Test Plugin 2",
                version="0.0.0",
                entrypoint="tests",
            )

            def register(self, api) -> None:
                api.register_operation(
                    PluginOperation(
                        plugin_id="uitest2",
                        capability="uitest2",
                        method="ui_describe",
                        handler=_describe,
                        minimum_profile=CapabilityProfile.MAINTAINER,
                        mutating=False,
                    )
                )
                api.register_operation(
                    PluginOperation(
                        plugin_id="uitest2",
                        capability="uitest2",
                        method="blocked_op",
                        handler=_blocked,
                        minimum_profile=CapabilityProfile.MAINTAINER,
                        mutating=True,
                    )
                )

        plugin_registry.register_plugin(_UiTestPlugin2())
        plugin_registry.register_policy_hook(
            plugin_id="uitest2",
            capability="uitest2",
            method="blocked_op",
            hook=lambda _ctx: SimpleNamespace(
                allowed=False, code="BLOCKED", message="Blocked by policy"
            ),
        )
        host._ctx.config.ui.tui_plugin_ui_allowlist = ["uitest2"]

        response = await _dispatch(
            host,
            CoreRequest(
                session_id="maintainer-session",
                capability="tui",
                method="api_call",
                params={
                    "method": "plugin_ui_invoke",
                    "kwargs": {
                        "project_id": host._ctx.active_project_id,
                        "plugin_id": "uitest2",
                        "action_id": "blocked",
                    },
                },
            ),
        )

        assert response.ok
        assert response.result is not None
        payload = response.result["value"]
        assert payload["ok"] is False
        assert payload["code"] == "BLOCKED"
        assert payload["message"] == "Blocked by policy"

    async def test_viewer_denied_before_api_dispatch(self, handle_host: tuple) -> None:
        host, _api = handle_host

        response = await _dispatch(
            host,
            CoreRequest(
                session_id="viewer-session",
                capability="settings",
                method="get",
                params={},
            ),
        )

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "AUTHORIZATION_DENIED"

    async def test_tui_api_call_submit_job_dispatches_to_real_api(
        self,
        handle_host: tuple,
    ) -> None:
        host, api = handle_host
        task = await api.create_task("TUI submit job")

        response = await _dispatch(
            host,
            CoreRequest(
                session_id="maintainer-session",
                capability="tui",
                method="api_call",
                params={
                    "method": "submit_job",
                    "kwargs": {"task_id": task.id, "action": "start_agent"},
                },
            ),
        )

        assert response.ok
        assert response.result is not None
        assert response.result["success"] is True
        payload = response.result["value"]
        assert payload["action"] == "start_agent"
        assert payload["job_id"]

    async def test_tui_api_call_github_sync_dispatches_to_typed_api(
        self,
        handle_host: tuple,
    ) -> None:
        host, _api = handle_host
        handler = AsyncMock(return_value={"success": True, "stats": {"inserted": 2}})
        registry = MagicMock()
        registry.resolve_operation.return_value = SimpleNamespace(handler=handler)
        host._ctx.plugin_registry = registry

        response = await _dispatch(
            host,
            CoreRequest(
                session_id="maintainer-session",
                capability="tui",
                method="api_call",
                params={
                    "method": "github_sync_issues",
                    "kwargs": {"project_id": "project-1", "repo_id": "repo-1"},
                },
            ),
        )

        assert response.ok
        assert response.result is not None
        assert response.result["success"] is True
        assert response.result["value"]["success"] is True
        assert response.result["value"]["stats"]["inserted"] == 2
        registry.resolve_operation.assert_called_once_with(
            GITHUB_CAPABILITY,
            GITHUB_METHOD_SYNC_ISSUES,
        )
        handler.assert_awaited_once_with(
            host._ctx,
            {"project_id": "project-1", "repo_id": "repo-1"},
        )

    async def test_tui_api_call_reconcile_running_tasks_returns_runtime_snapshots(
        self,
        handle_host: tuple,
    ) -> None:
        host, api = handle_host
        api.reconcile_running_tasks = AsyncMock(
            return_value=[{"task_id": "task-1", "runtime": {"is_running": True}}]
        )

        response = await _dispatch(
            host,
            CoreRequest(
                session_id="maintainer-session",
                capability="tui",
                method="api_call",
                params={
                    "method": "reconcile_running_tasks",
                    "kwargs": {"task_ids": ["task-1", "task-1"]},
                },
            ),
        )

        assert response.ok
        assert response.result is not None
        assert response.result["success"] is True
        assert response.result["value"] == [{"task_id": "task-1", "runtime": {"is_running": True}}]
        api.reconcile_running_tasks.assert_awaited_once_with(["task-1"])

    async def test_viewer_denied_for_tui_api_dispatch(self, handle_host: tuple) -> None:
        host, _api = handle_host

        response = await _dispatch(
            host,
            CoreRequest(
                session_id="viewer-session",
                capability="tui",
                method="api_call",
                params={"method": "list_tasks", "kwargs": {}},
            ),
        )

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "AUTHORIZATION_DENIED"


class TestNoApi:
    """Built-in dispatch map requires an API boundary on AppContext."""

    async def test_request_without_api_attribute_returns_unknown_method(self) -> None:
        host = CoreHost()
        host._ctx = cast("AppContext", object())
        host.register_session("session-1", "maintainer")

        response = await _dispatch(
            host,
            CoreRequest(
                session_id="session-1",
                capability="tasks",
                method="list",
                params={},
            ),
        )

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "UNKNOWN_METHOD"
