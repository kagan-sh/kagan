"""Tests for CoreHost request dispatch behavior."""

from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING

import pytest

from kagan.core.ipc.contracts import CoreRequest

if TYPE_CHECKING:
    from kagan.core.host import CoreHost


async def _dispatch_request(host: CoreHost, request: CoreRequest):
    return await host.handle_request(request)


def _set_dispatch_map(monkeypatch: pytest.MonkeyPatch, dispatch_map: dict[tuple[str, str], object]):
    monkeypatch.setattr("kagan.core.host._REQUEST_DISPATCH_MAP", dispatch_map)


class TestCoreHostDispatch:
    """Test CoreHost.handle_request dispatch logic without starting IPC."""

    @pytest.fixture()
    def host(self):
        from kagan.core.host import CoreHost

        return CoreHost()

    @pytest.mark.asyncio()
    async def test_dispatch_unknown_method_returns_failure(self, host):
        """Unknown capability.method returns UNKNOWN_METHOD error."""
        host._ctx = object()

        request = CoreRequest(
            session_id="test-session",
            session_profile="maintainer",
            capability="nonexistent",
            method="nope",
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "UNKNOWN_METHOD"
        assert "nonexistent.nope" in response.error.message

    @pytest.mark.asyncio()
    async def test_dispatch_without_context_returns_not_ready(self, host):
        """Request before context init returns NOT_READY error."""
        request = CoreRequest(
            session_id="test-session",
            capability="tasks",
            method="list",
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "NOT_READY"

    @pytest.mark.asyncio()
    async def test_dispatch_routes_to_request_handler(self, host, monkeypatch):
        """Known capability.method dispatches to a request handler."""
        call_log = []

        async def mock_adapter(api, params):
            del api
            call_log.append(params)
            return {"handled": True}

        _set_dispatch_map(monkeypatch, {("test", "action"): mock_adapter})
        host._ctx = SimpleNamespace(api=object())

        request = CoreRequest(
            session_id="test-session",
            session_profile="maintainer",
            capability="test",
            method="action",
            params={"key": "value"},
        )
        response = await _dispatch_request(host, request)

        assert response.ok
        assert response.result == {"handled": True}
        assert call_log == [{"key": "value"}]

    @pytest.mark.asyncio()
    async def test_dispatch_adapter_key_error_returns_invalid_params(self, host, monkeypatch):
        """KeyError from adapter returns INVALID_PARAMS."""

        async def bad_adapter(api, params):
            del api
            _ = params["missing_key"]
            return {}

        _set_dispatch_map(monkeypatch, {("test", "bad"): bad_adapter})
        host._ctx = SimpleNamespace(api=object())

        request = CoreRequest(
            session_id="test-session",
            session_profile="maintainer",
            capability="test",
            method="bad",
            params={},
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "INVALID_PARAMS"

    @pytest.mark.asyncio()
    async def test_dispatch_adapter_value_error_returns_invalid_params(self, host, monkeypatch):
        """ValueError from adapter returns INVALID_PARAMS with actionable message."""

        async def bad_adapter(api, params):
            del api, params
            raise ValueError("AUTO/PAIR are task_type values; use task_type")

        _set_dispatch_map(monkeypatch, {("test", "bad_value"): bad_adapter})
        host._ctx = SimpleNamespace(api=object())

        request = CoreRequest(
            session_id="test-session",
            session_profile="maintainer",
            capability="test",
            method="bad_value",
            params={},
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "INVALID_PARAMS"
        assert "task_type" in response.error.message

    @pytest.mark.asyncio()
    async def test_dispatch_adapter_exception_returns_internal_error(self, host, monkeypatch):
        """Unexpected exception from adapter returns INTERNAL_ERROR."""

        async def failing_adapter(api, params):
            del api, params
            msg = "something broke"
            raise RuntimeError(msg)

        _set_dispatch_map(monkeypatch, {("test", "fail"): failing_adapter})
        host._ctx = SimpleNamespace(api=object())

        request = CoreRequest(
            session_id="test-session",
            session_profile="maintainer",
            capability="test",
            method="fail",
        )
        response = await _dispatch_request(host, request)

        assert not response.ok
        assert response.error is not None
        assert response.error.code == "INTERNAL_ERROR"

    @pytest.mark.asyncio()
    async def test_mutating_replay_with_same_idempotency_key_is_deduplicated(
        self, host, monkeypatch
    ):
        """Duplicate mutating replay returns cached response and executes only once."""
        call_count = 0

        async def mutate_adapter(api, params):
            del api
            nonlocal call_count
            call_count += 1
            return {"success": True, "call_count": call_count, "params": params}

        _set_dispatch_map(monkeypatch, {("tasks", "create"): mutate_adapter})
        host._ctx = SimpleNamespace(api=object())

        request_1 = CoreRequest(
            session_id="test-session",
            session_profile="maintainer",
            capability="tasks",
            method="create",
            params={"title": "dedup me"},
            idempotency_key="mutate-key-1",
        )
        request_2 = CoreRequest(
            session_id="test-session",
            session_profile="maintainer",
            capability="tasks",
            method="create",
            params={"title": "dedup me"},
            idempotency_key="mutate-key-1",
        )

        response_1 = await _dispatch_request(host, request_1)
        response_2 = await _dispatch_request(host, request_2)

        assert response_1.ok
        assert response_2.ok
        assert response_1.result == response_2.result
        assert call_count == 1
        assert response_1.request_id == request_1.request_id
        assert response_2.request_id == request_2.request_id

    @pytest.mark.asyncio()
    async def test_non_mutating_call_with_idempotency_key_is_not_deduplicated(
        self, host, monkeypatch
    ):
        """Read/query methods remain unaffected even when idempotency_key is provided."""
        call_count = 0

        async def query_adapter(api, params):
            del api, params
            nonlocal call_count
            call_count += 1
            return {"call_count": call_count}

        _set_dispatch_map(monkeypatch, {("tasks", "list"): query_adapter})
        host._ctx = SimpleNamespace(api=object())

        request_1 = CoreRequest(
            session_id="test-session",
            session_profile="maintainer",
            capability="tasks",
            method="list",
            idempotency_key="query-key-1",
        )
        request_2 = CoreRequest(
            session_id="test-session",
            session_profile="maintainer",
            capability="tasks",
            method="list",
            idempotency_key="query-key-1",
        )

        response_1 = await _dispatch_request(host, request_1)
        response_2 = await _dispatch_request(host, request_2)

        assert response_1.ok
        assert response_2.ok
        assert response_1.result == {"call_count": 1}
        assert response_2.result == {"call_count": 2}
        assert call_count == 2
