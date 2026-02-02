"""Minimal JSON-RPC 2.0 handling for ACP communication."""

from __future__ import annotations

import asyncio
import inspect
import json
from asyncio import Future
from typing import TYPE_CHECKING, Any
from weakref import WeakValueDictionary

if TYPE_CHECKING:
    from collections.abc import Callable

# Type aliases
type JSONType = dict[str, Any] | list[Any] | str | int | float | bool | None


class RPCError(Exception):
    """Error raised by RPC methods or received from remote."""

    def __init__(self, message: str, code: int = -32603, data: Any = None) -> None:
        self.message = message
        self.code = code
        self.data = data
        super().__init__(f"[{code}] {message}")


# --- Server (incoming requests) ---


class Server:
    """Simple JSON-RPC server for dispatching incoming requests."""

    def __init__(self) -> None:
        self._methods: dict[str, Callable[..., Any]] = {}

    def register(self, name: str, handler: Callable[..., Any]) -> None:
        """Register a method handler."""
        self._methods[name] = handler

    async def call(self, data: dict[str, Any]) -> dict[str, Any] | None:
        """Dispatch a JSON-RPC request and return response dict.

        Alias: dispatch() - kept for backward compatibility with tests.
        """
        request_id = data.get("id")
        is_notification = request_id is None

        try:
            # Validate
            if data.get("jsonrpc") != "2.0":
                raise RPCError("Invalid jsonrpc version", -32600)

            method_name = data.get("method")
            if not isinstance(method_name, str):
                raise RPCError("Missing method", -32600)

            handler = self._methods.get(method_name)
            if handler is None:
                raise RPCError(f"Method not found: {method_name}", -32601)

            # Extract params
            params = data.get("params", {})
            if isinstance(params, list):
                # Convert positional to keyword args
                sig = inspect.signature(handler)
                param_names = [p for p in sig.parameters if p != "self"]
                params = dict(zip(param_names, params, strict=False))

            # Call handler
            if inspect.iscoroutinefunction(handler):
                result = await handler(**params)
            else:
                result = handler(**params)

            if is_notification:
                return None
            return {"jsonrpc": "2.0", "result": result, "id": request_id}

        except RPCError as e:
            if is_notification:
                return None
            error = {"code": e.code, "message": e.message}
            if e.data is not None:
                error["data"] = e.data
            return {"jsonrpc": "2.0", "error": error, "id": request_id}
        except Exception as e:
            if is_notification:
                return None
            return {
                "jsonrpc": "2.0",
                "error": {"code": -32603, "message": str(e)},
                "id": request_id,
            }


# --- Client (outgoing requests) ---


class PendingCall:
    """A pending RPC call waiting for response."""

    def __init__(self, call_id: int, method: str, params: dict[str, Any]) -> None:
        self.id = call_id
        self.method = method
        self.params = params
        self._future: Future[Any] | None = None

    @property
    def future(self) -> Future[Any]:
        if self._future is None:
            self._future = asyncio.get_running_loop().create_future()
        return self._future

    def as_dict(self) -> dict[str, Any]:
        """Convert to JSON-RPC request dict."""
        req: dict[str, Any] = {"jsonrpc": "2.0", "method": self.method, "id": self.id}
        if self.params:
            req["params"] = self.params
        return req

    async def wait(self, timeout: float | None = None) -> Any:
        """Wait for the response."""
        if timeout is not None:
            return await asyncio.wait_for(self.future, timeout)
        return await self.future


class Client:
    """Simple JSON-RPC client for outgoing requests."""

    def __init__(self) -> None:
        self._next_id = 0
        self._pending: WeakValueDictionary[int, PendingCall] = WeakValueDictionary()
        self._send_callback: Callable[[bytes], None] | None = None

    def set_sender(self, callback: Callable[[bytes], None]) -> None:
        """Set callback for sending requests."""
        self._send_callback = callback

    def call(self, method: str, **params: Any) -> PendingCall:
        """Create and send an RPC call."""
        self._next_id += 1
        call = PendingCall(self._next_id, method, params)
        self._pending[call.id] = call
        if self._send_callback:
            self._send_callback(json.dumps(call.as_dict()).encode() + b"\n")
        return call

    def notify(self, method: str, **params: Any) -> None:
        """Send a notification (no response expected)."""
        req: dict[str, Any] = {"jsonrpc": "2.0", "method": method}
        if params:
            req["params"] = params
        if self._send_callback:
            self._send_callback(json.dumps(req).encode() + b"\n")

    def handle_response(self, data: dict[str, Any]) -> None:
        """Process a response from the remote."""
        call_id = data.get("id")
        if not isinstance(call_id, int):
            return

        call = self._pending.get(call_id)
        if call is None:
            return

        if "error" in data:
            err = data["error"]
            call.future.set_exception(
                RPCError(
                    message=str(err.get("message", "Unknown error")),
                    code=int(err.get("code", -1)),
                    data=err.get("data"),
                )
            )
        elif "result" in data:
            call.future.set_result(data["result"])
