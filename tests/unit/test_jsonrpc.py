"""Tests for JSON-RPC 2.0 framework - core classes."""

from __future__ import annotations

import pytest

from kagan.jsonrpc import (
    API,
    EXPOSE_ATTR,
    InvalidParams,
    Method,
    MethodCall,
    Request,
    expose,
)

pytestmark = pytest.mark.unit


class TestErrorClasses:
    def test_as_json_object(self):
        err = InvalidParams("missing field", data={"field": "name"})
        obj = err.as_json_object()
        assert obj["code"] == -32602
        assert obj["data"] == {"field": "name"}


class TestExposeDecorator:
    @pytest.mark.parametrize(
        ("name", "prefix", "func_name", "expected"),
        [
            ("my/method", None, "example", "my/method"),
            (None, None, "get_user_data", "get/user/data"),
            ("read", "file", "read", "file/read"),
        ],
    )
    def test_expose_naming(self, name, prefix, func_name, expected):
        """Test @expose decorator naming conventions."""
        kwargs = {}
        if name:
            kwargs["name"] = name
        if prefix:
            kwargs["prefix"] = prefix

        @expose(**kwargs) if kwargs else expose()
        def fn():
            pass

        fn.__name__ = func_name
        # Re-apply decorator for func_name test
        if not name and not prefix:
            fn = expose()(lambda: None)
            fn.__name__ = func_name
            setattr(fn, EXPOSE_ATTR, func_name.replace("_", "/"))

        # Direct test
        if name == "my/method":

            @expose("my/method")
            def example():
                pass

            assert getattr(example, EXPOSE_ATTR) == "my/method"
        elif prefix == "file":

            @expose(name="read", prefix="file")
            def read():
                pass

            assert getattr(read, EXPOSE_ATTR) == "file/read"
        else:

            @expose()
            def get_user_data():
                pass

            assert getattr(get_user_data, EXPOSE_ATTR) == "get/user/data"


class TestMethodFromCallable:
    def test_extracts_param_types(self):
        def example(name: str, count: int) -> str:
            return ""

        method = Method.from_callable("test", example)
        assert method.param_types == {"name": str, "count": int}

    def test_skips_self_param(self):
        class Example:
            def method(self, value: int) -> None:
                pass

        method = Method.from_callable("test", Example().method)
        assert "self" not in method.param_types

    @pytest.mark.parametrize(
        ("is_async_func", "expected"),
        [(True, True), (False, False)],
    )
    def test_detects_async(self, is_async_func, expected):
        if is_async_func:

            async def fn():
                pass
        else:

            def fn():
                pass

        method = Method.from_callable("test", fn)
        assert method.is_async is expected


class TestMethodCall:
    @pytest.mark.parametrize(
        ("method", "id_val", "params", "has_params", "has_id"),
        [
            ("test", 1, None, False, True),
            ("add", 2, {"a": 1, "b": 2}, True, True),
            ("notify", None, None, False, False),
        ],
    )
    def test_as_json_object(self, method, id_val, params, has_params, has_id):
        call = MethodCall(method=method, id=id_val, parameters=params)
        obj = call.as_json_object
        assert obj["method"] == method
        assert ("params" in obj) == has_params
        assert ("id" in obj) == has_id


class TestRequest:
    def test_body_variations(self):
        api = API()
        req = Request(api, callback=None)
        assert req.body is None

        req.add(MethodCall(method="test", id=1))
        assert req.body == {"jsonrpc": "2.0", "method": "test", "id": 1}

        req.add(MethodCall(method="test2", id=2))
        assert isinstance(req.body, list) and len(req.body) == 2
