"""High-signal integration tests for CoreHost API dispatch."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

import pytest
from _api_helpers import build_api

from kagan.core.host import CoreHost
from kagan.core.ipc.contracts import CoreRequest

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
