"""JSON-RPC 2.0 framework for agent communication."""

from __future__ import annotations

import asyncio
import inspect
import json
import logging
from asyncio import Future
from dataclasses import dataclass, field
from enum import IntEnum
from typing import TYPE_CHECKING, Any, ParamSpec, TypeVar, get_type_hints
from weakref import WeakValueDictionary

from typeguard import check_type

if TYPE_CHECKING:
    from collections.abc import Callable
    from types import TracebackType

log = logging.getLogger("jsonrpc")

# Type aliases (Python 3.12+)
type JSONValue = str | int | float | bool | None
type JSONType = dict[str, JSONType] | list[JSONType] | str | int | float | bool | None
type JSONObject = dict[str, JSONType]
type JSONList = list[JSONType]

T = TypeVar("T")
P = ParamSpec("P")
ReturnType = TypeVar("ReturnType")


# --- Expose decorator ---


EXPOSE_ATTR = "_jsonrpc_expose"


def expose(name: str | None = None, prefix: str | None = None) -> Callable[[T], T]:
    """Decorator to mark a method as callable via RPC.

    Args:
        name: RPC method name (defaults to function name with underscores as slashes).
        prefix: Optional prefix for the method name.

    Returns:
        Decorated function with _jsonrpc_expose attribute.
    """

    def decorator(func: T) -> T:
        func_name: str = getattr(func, "__name__", "unknown")
        method_name = name if name is not None else func_name.replace("_", "/")
        if prefix:
            method_name = f"{prefix}/{method_name}"
        setattr(func, EXPOSE_ATTR, method_name)
        return func

    return decorator


# --- Error classes ---


class ErrorCode(IntEnum):
    """JSON-RPC 2.0 error codes."""

    PARSE_ERROR = -32700
    INVALID_REQUEST = -32600
    METHOD_NOT_FOUND = -32601
    INVALID_PARAMS = -32602
    INTERNAL_ERROR = -32603


class JSONRPCError(Exception):
    """Base JSON-RPC error."""

    code: ErrorCode
    message: str
    data: JSONType

    def __init__(
        self, message: str | None = None, data: JSONType = None, code: ErrorCode | None = None
    ) -> None:
        self.message = message or self.__class__.message
        self.data = data
        if code is not None:
            self.code = code
        super().__init__(self.message)

    def as_json_object(self) -> JSONObject:
        """Serialize error to JSON-RPC format."""
        obj: JSONObject = {"code": int(self.code), "message": self.message}
        if self.data is not None:
            obj["data"] = self.data
        return obj


class ParseError(JSONRPCError):
    """Invalid JSON received."""

    code = ErrorCode.PARSE_ERROR
    message = "Parse error"


class InvalidRequest(JSONRPCError):
    """Invalid JSON-RPC request."""

    code = ErrorCode.INVALID_REQUEST
    message = "Invalid request"


class MethodNotFound(JSONRPCError):
    """Method not found."""

    code = ErrorCode.METHOD_NOT_FOUND
    message = "Method not found"


class InvalidParams(JSONRPCError):
    """Invalid method parameters."""

    code = ErrorCode.INVALID_PARAMS
    message = "Invalid params"


class InternalError(JSONRPCError):
    """Internal server error."""

    code = ErrorCode.INTERNAL_ERROR
    message = "Internal error"


class APIError(Exception):
    """Error received from remote API."""

    def __init__(self, code: int, message: str, data: JSONType = None) -> None:
        self.code = code
        self.message = message
        self.data = data
        super().__init__(f"[{code}] {message}")


# --- Method wrapper ---


@dataclass
class Method:
    """Wrapper for a registered RPC method."""

    name: str
    func: Callable[..., Any]
    is_async: bool
    param_types: dict[str, type]

    @classmethod
    def from_callable(cls, name: str, func: Callable[..., Any]) -> Method:
        """Create Method from a callable."""
        sig = inspect.signature(func)
        hints = get_type_hints(func)

        param_types: dict[str, type] = {}
        for param_name, _param in sig.parameters.items():
            if param_name == "self":
                continue
            if param_name in hints:
                param_types[param_name] = hints[param_name]

        return cls(
            name=name,
            func=func,
            is_async=inspect.iscoroutinefunction(func),
            param_types=param_types,
        )


# --- Server class ---


class Server:
    """JSON-RPC 2.0 server for handling incoming requests."""

    def __init__(self) -> None:
        self._methods: dict[str, Method] = {}

    def method(
        self, name: str | None = None, prefix: str | None = None
    ) -> Callable[[Callable[P, T]], Callable[P, T]]:
        """Decorator to register a method.

        Args:
            name: RPC method name (defaults to function name).
            prefix: Optional prefix for the method name.
        """

        def decorator(func: Callable[P, T]) -> Callable[P, T]:
            method_name = name or func.__name__
            if prefix:
                method_name = f"{prefix}/{method_name}"
            self._methods[method_name] = Method.from_callable(method_name, func)
            return func

        return decorator

    def expose_instance(self, instance: object) -> None:
        """Auto-discover and register @expose decorated methods from an instance."""
        for attr_name in dir(instance):
            if attr_name.startswith("_"):
                continue
            attr = getattr(instance, attr_name, None)
            if callable(attr) and hasattr(attr, EXPOSE_ATTR):
                method_name = getattr(attr, EXPOSE_ATTR)
                self._methods[method_name] = Method.from_callable(method_name, attr)
                log.debug("Registered RPC method: %s", method_name)

    async def call(self, data: str | bytes | JSONType) -> JSONType:
        """Dispatch a JSON-RPC request (single or batch).

        Args:
            data: JSON string, bytes, or already-parsed JSON.

        Returns:
            Response object or list of response objects, or None for notifications.
        """
        # Parse if needed
        if isinstance(data, (str, bytes)):
            try:
                parsed = json.loads(data)
            except json.JSONDecodeError as e:
                return {
                    "jsonrpc": "2.0",
                    "error": ParseError(str(e)).as_json_object(),
                    "id": None,
                }
        else:
            parsed = data

        # Batch request
        if isinstance(parsed, list):
            if not parsed:
                return {
                    "jsonrpc": "2.0",
                    "error": InvalidRequest("Empty batch").as_json_object(),
                    "id": None,
                }
            responses = []
            for item in parsed:
                response = await self._dispatch_object(item)
                if response is not None:
                    responses.append(response)
            return responses if responses else None

        # Single request
        return await self._dispatch_object(parsed)

    async def _dispatch_object(self, data: JSONType) -> JSONObject | None:
        """Route a single request object to its handler."""
        if not isinstance(data, dict):
            return {
                "jsonrpc": "2.0",
                "error": InvalidRequest("Request must be an object").as_json_object(),
                "id": None,
            }

        raw_id = data.get("id")
        # Cast to valid ID type (JSON-RPC allows int, str, or null)
        request_id: int | str | None = (
            raw_id if isinstance(raw_id, (int, str)) or raw_id is None else None
        )
        is_notification = request_id is None

        try:
            result = await self._dispatch_object_call(request_id, data)
            if is_notification:
                return None
            return {"jsonrpc": "2.0", "result": result, "id": request_id}
        except JSONRPCError as e:
            if is_notification:
                return None
            return {"jsonrpc": "2.0", "error": e.as_json_object(), "id": request_id}
        except Exception as e:
            log.exception("Internal error in RPC method")
            if is_notification:
                return None
            return {
                "jsonrpc": "2.0",
                "error": InternalError(str(e)).as_json_object(),
                "id": request_id,
            }

    async def _dispatch_object_call(
        self, request_id: int | str | None, data: dict[str, Any]
    ) -> JSONType:
        """Validate request, extract params, and call the method."""
        # Validate jsonrpc version
        if data.get("jsonrpc") != "2.0":
            raise InvalidRequest("Missing or invalid 'jsonrpc' field")

        # Get method name
        method_name = data.get("method")
        if not isinstance(method_name, str):
            raise InvalidRequest("Missing or invalid 'method' field")

        # Find method
        method = self._methods.get(method_name)
        if method is None:
            raise MethodNotFound(f"Method '{method_name}' not found")

        # Extract params
        params = data.get("params", {})
        if isinstance(params, list):
            # Positional params - convert to kwargs
            sig = inspect.signature(method.func)
            param_names = [p for p in sig.parameters if p != "self"]
            if len(params) > len(param_names):
                raise InvalidParams("Too many positional parameters")
            params = dict(zip(param_names, params, strict=False))
        elif not isinstance(params, dict):
            raise InvalidParams("'params' must be an object or array")

        # Type check params
        for param_name, expected_type in method.param_types.items():
            if param_name in params:
                try:
                    check_type(params[param_name], expected_type)
                except Exception as e:
                    raise InvalidParams(f"Parameter '{param_name}': {e}") from e

        # Call method
        try:
            if method.is_async:
                result = await method.func(**params)
            else:
                result = method.func(**params)
            return result
        except TypeError as e:
            raise InvalidParams(str(e)) from e


# --- MethodCall class ---


@dataclass
class MethodCall[ReturnType]:
    """Represents an outgoing RPC method call."""

    method: str
    id: int | None
    parameters: dict[str, Any] = field(default_factory=dict)
    _future: Future[ReturnType] | None = field(default=None, repr=False)

    @property
    def future(self) -> Future[ReturnType]:
        """Lazily create and return the future for this call."""
        if self._future is None:
            try:
                loop = asyncio.get_running_loop()
            except RuntimeError:
                # No running loop - create a new one for sync context
                loop = asyncio.new_event_loop()
            self._future = loop.create_future()
        return self._future

    @property
    def as_json_object(self) -> JSONObject:
        """Serialize to JSON-RPC request format."""
        obj: JSONObject = {"jsonrpc": "2.0", "method": self.method}
        if self.parameters:
            obj["params"] = self.parameters
        if self.id is not None:
            obj["id"] = self.id
        return obj

    async def wait(self, timeout: float | None = None) -> ReturnType:
        """Wait for the response.

        Args:
            timeout: Optional timeout in seconds.

        Returns:
            The result from the remote method.

        Raises:
            APIError: If the remote returned an error.
            asyncio.TimeoutError: If timeout exceeded.
        """
        if timeout is not None:
            return await asyncio.wait_for(self.future, timeout)
        return await self.future


# --- Request context manager ---


class Request:
    """Context manager for collecting method calls into a batch."""

    def __init__(self, api: API, callback: Callable[[Request], None] | None) -> None:
        """Initialize request.

        Args:
            api: The parent API instance.
            callback: Callback to invoke on context exit.
        """
        self.api = api
        self._calls: list[MethodCall[Any]] = []
        self._callback = callback

    def add(self, call: MethodCall[Any]) -> None:
        """Add a method call to this request."""
        self._calls.append(call)

    def __enter__(self) -> Request:
        """Enter context, register with API."""
        self.api._requests.append(self)
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        """Exit context, invoke callback and cleanup."""
        self.api._requests.pop()
        if self._callback is not None:
            self._callback(self)

    @property
    def body(self) -> JSONObject | JSONList | None:
        """Get the request body (single call or batch).

        Returns:
            The body or None if no calls.
        """
        if not self._calls:
            return None
        if len(self._calls) == 1:
            return self._calls[0].as_json_object
        return [c.as_json_object for c in self._calls]

    @property
    def body_json(self) -> bytes:
        """Get the request body as JSON bytes."""
        return json.dumps(self.body).encode("utf-8")

    @property
    def calls(self) -> list[MethodCall[Any]]:
        """Get all calls in this request."""
        return self._calls


# --- API class ---


class API:
    """JSON-RPC 2.0 client for making outgoing requests."""

    def __init__(self) -> None:
        self._request_id: int = 0
        self._requests: list[Request] = []
        self._calls: WeakValueDictionary[int, MethodCall[Any]] = WeakValueDictionary()

    def _next_id(self) -> int:
        """Generate the next request ID."""
        self._request_id += 1
        return self._request_id

    def request(self, callback: Callable[[Request], None] | None = None) -> Request:
        """Create a request context manager.

        Args:
            callback: Optional callback to invoke with Request on exit.

        Returns:
            Request context manager.
        """
        return Request(self, callback)

    @property
    def current_request(self) -> Request | None:
        """Get the current request context, if any."""
        return self._requests[-1] if self._requests else None

    def method(
        self, name: str | None = None, prefix: str | None = None, notification: bool = False
    ) -> Callable[[Callable[P, T]], Callable[P, MethodCall[T]]]:
        """Decorator to create an outgoing RPC method.

        Args:
            name: RPC method name (defaults to function name).
            prefix: Optional prefix for the method name.
            notification: If True, don't expect a response.
        """

        def decorator(func: Callable[P, T]) -> Callable[P, MethodCall[T]]:
            method_name = name or func.__name__
            if prefix:
                method_name = f"{prefix}/{method_name}"

            sig = inspect.signature(func)
            param_names = [p for p in sig.parameters if p != "self"]

            def wrapper(*args: P.args, **kwargs: P.kwargs) -> MethodCall[T]:
                # Build params dict from args/kwargs
                params: dict[str, Any] = {}
                for i, arg in enumerate(args):
                    if i < len(param_names):
                        params[param_names[i]] = arg
                params.update(kwargs)

                # Create method call
                call_id = None if notification else self._next_id()
                call: MethodCall[T] = MethodCall(
                    method=method_name,
                    id=call_id,
                    parameters=params,
                )

                # Track call for response resolution
                if call_id is not None:
                    self._calls[call_id] = call

                # Add to current request if in context
                if self._requests:
                    self._requests[-1].add(call)

                return call

            wrapper.__name__ = func.__name__
            wrapper.__doc__ = func.__doc__
            return wrapper

        return decorator

    def notification(
        self, name: str | None = None, prefix: str | None = None
    ) -> Callable[[Callable[P, T]], Callable[P, MethodCall[T]]]:
        """Decorator for notification methods (no response expected).

        Args:
            name: RPC method name (defaults to function name).
            prefix: Optional prefix for the method name.
        """
        return self.method(name=name, prefix=prefix, notification=True)

    def process_response(self, response: str | bytes | JSONType) -> None:
        """Process a response from the remote, resolving pending futures.

        Args:
            response: JSON string, bytes, or parsed response object.
        """
        # Parse if needed
        if isinstance(response, (str, bytes)):
            try:
                parsed = json.loads(response)
            except json.JSONDecodeError:
                log.error("Failed to parse response JSON")
                return
        else:
            parsed = response

        # Handle batch response
        if isinstance(parsed, list):
            for item in parsed:
                self._process_single_response(item)
        else:
            self._process_single_response(parsed)

    def _process_single_response(self, response: JSONType) -> None:
        """Process a single response object."""
        if not isinstance(response, dict):
            log.warning("Invalid response: not an object")
            return

        raw_id = response.get("id")
        if raw_id is None:
            return  # Notification response or error without id

        # Request IDs must be int for our tracking
        if not isinstance(raw_id, int):
            log.warning("Response ID is not an integer: %s", raw_id)
            return

        call = self._calls.get(raw_id)
        if call is None:
            log.warning("Received response for unknown request ID: %s", raw_id)
            return

        if "error" in response:
            error = response["error"]
            if isinstance(error, dict):
                raw_code = error.get("code", -1)
                raw_message = error.get("message", "Unknown error")
                exc = APIError(
                    code=int(raw_code) if isinstance(raw_code, (int, float)) else -1,
                    message=str(raw_message),
                    data=error.get("data"),
                )
            else:
                exc = APIError(code=-1, message=str(error))
            call.future.set_exception(exc)
        elif "result" in response:
            call.future.set_result(response["result"])
        else:
            log.warning("Response has neither 'result' nor 'error': %s", response)
