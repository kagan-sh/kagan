"""Tests for JSON-RPC 2.0 framework - API and Server."""

from __future__ import annotations

from typing import Any, cast

import pytest

from kagan.jsonrpc import API, Server

pytestmark = pytest.mark.unit


class TestAPI:
    def test_next_id_increments(self):
        api = API()
        id1 = api._next_id()
        assert api._next_id() == id1 + 1

    def test_method_decorator(self):
        api = API()

        @api.method()
        def add(a: int, b: int) -> int:
            return a + b

        call = add(1, 2)
        assert call.method == "add"
        assert call.parameters == {"a": 1, "b": 2}

    def test_process_response_success(self):
        api = API()

        @api.method()
        def test_method() -> int:
            return 0

        call = test_method()
        api.process_response({"jsonrpc": "2.0", "result": 42, "id": call.id})
        assert call.future.result() == 42

    def test_process_response_error(self):
        api = API()

        @api.method()
        def test_method() -> int:
            return 0

        call = test_method()
        api.process_response(
            {"jsonrpc": "2.0", "error": {"code": -32600, "message": "Bad"}, "id": call.id}
        )
        with pytest.raises(Exception, match=r"-32600|Bad"):
            call.future.result()


class TestServer:
    @pytest.mark.parametrize(
        ("name", "prefix", "expected_key"),
        [
            (None, None, "add"),
            ("math/add", None, "math/add"),
            (None, "v1", "v1/add"),
        ],
    )
    def test_register_method(self, name, prefix, expected_key):
        server = Server()
        kwargs = {}
        if name:
            kwargs["name"] = name
        if prefix:
            kwargs["prefix"] = prefix

        @server.method(**kwargs)
        def add(a: int, b: int) -> int:
            return a + b

        assert expected_key in server._methods

    @pytest.mark.parametrize(
        ("req_data", "expected_code"),
        [
            (b"not json", -32700),
            ({"method": "test", "id": 1}, -32600),
            ({"jsonrpc": "2.0", "method": "nonexistent", "id": 1}, -32601),
            ([], -32600),
        ],
    )
    async def test_call_errors(self, req_data, expected_code):
        server = Server()
        response = cast("dict[str, Any]", await server.call(req_data))
        assert response["error"]["code"] == expected_code

    async def test_call_success(self):
        server = Server()

        @server.method()
        def add(a: int, b: int) -> int:
            return a + b

        response = cast(
            "dict[str, Any]",
            await server.call(
                {"jsonrpc": "2.0", "method": "add", "params": {"a": 1, "b": 2}, "id": 1}
            ),
        )
        assert response["result"] == 3

    async def test_call_batch(self):
        server = Server()

        @server.method()
        def double(n: int) -> int:
            return n * 2

        response = cast(
            "list[dict[str, Any]]",
            await server.call(
                [
                    {"jsonrpc": "2.0", "method": "double", "params": {"n": 1}, "id": 1},
                    {"jsonrpc": "2.0", "method": "double", "params": {"n": 2}, "id": 2},
                ]
            ),
        )
        assert {r["id"]: r["result"] for r in response} == {1: 2, 2: 4}

    async def test_notification_no_response(self):
        server = Server()
        called = []

        @server.method()
        def notify(msg: str) -> None:
            called.append(msg)

        response = await server.call(
            {"jsonrpc": "2.0", "method": "notify", "params": {"msg": "hello"}}
        )
        assert response is None
        assert called == ["hello"]
