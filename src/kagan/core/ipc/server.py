"""IPC server that accepts connections, validates tokens, and dispatches requests."""

from __future__ import annotations

import contextlib
import json
import logging
import secrets
from typing import TYPE_CHECKING

from kagan.core.ipc.constants import MAX_LINE_BYTES
from kagan.core.ipc.contracts import CoreRequest, CoreResponse
from kagan.core.ipc.transports import DefaultTransport, TCPLoopbackTransport, UnixSocketTransport

if TYPE_CHECKING:
    import asyncio
    from collections.abc import Callable, Coroutine
    from typing import Any

    from kagan.core.ipc.transports import ServerHandle

    RequestHandler = Callable[[CoreRequest], Coroutine[Any, Any, CoreResponse]]

logger = logging.getLogger(__name__)

_TOKEN_BYTES = 32

_TRANSPORT_MAP: dict[str, type[TCPLoopbackTransport] | type[UnixSocketTransport]] = {
    "tcp": TCPLoopbackTransport,
    "socket": UnixSocketTransport,
}


def _transport_for_preference(preference: str) -> TCPLoopbackTransport | UnixSocketTransport:
    """Instantiate a transport from a preference string."""
    cls = _TRANSPORT_MAP.get(preference)
    if cls is not None:
        return cls()
    return DefaultTransport()


class IPCServer:
    """Asynchronous IPC server for Kagan core.

    Accepts connections over the configured transport, validates a bearer
    token on every request, and delegates to a pluggable request handler.

    Usage::

        async def handle(req: CoreRequest) -> CoreResponse:
            return CoreResponse.success(req.request_id, result={"echo": True})


        server = IPCServer(handler=handle)
        await server.start()
        ...
        await server.stop()
    """

    def __init__(
        self,
        handler: RequestHandler,
        *,
        transport: TCPLoopbackTransport | UnixSocketTransport | None = None,
        transport_preference: str = "auto",
        token: str | None = None,
        on_client_connect: Callable[[], None] | None = None,
        on_client_disconnect: Callable[[], None] | None = None,
    ) -> None:
        self._handler = handler
        self._transport = transport or _transport_for_preference(transport_preference)
        self._token = token or secrets.token_hex(_TOKEN_BYTES)
        self._on_client_connect = on_client_connect
        self._on_client_disconnect = on_client_disconnect
        if isinstance(self._transport, TCPLoopbackTransport):
            self._transport.set_handshake_token(self._token)
        self._handle: ServerHandle | None = None

    @property
    def token(self) -> str:
        """Bearer token that clients must include in every request."""
        return self._token

    @property
    def handle(self) -> ServerHandle | None:
        """The server handle, available after ``start()``."""
        return self._handle

    @property
    def is_running(self) -> bool:
        """Whether the server is currently listening."""
        return self._handle is not None

    async def start(self) -> ServerHandle:
        """Start the server and begin accepting connections.

        Returns:
            A ``ServerHandle`` describing the listening endpoint.
        """
        if self._handle is not None:
            msg = "Server is already running"
            raise RuntimeError(msg)

        self._handle = await self._transport.start_server(self._client_connected)
        logger.info(
            "IPC server started: transport=%s address=%s port=%s",
            self._handle.transport_type,
            self._handle.address,
            self._handle.port,
        )
        return self._handle

    async def stop(self) -> None:
        """Gracefully shut down the server."""
        if self._handle is None:
            return
        if self._handle.close is not None:
            await self._handle.close()
        self._handle = None
        logger.info("IPC server stopped")

    async def _client_connected(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
    ) -> None:
        """Handle a single client connection (one or more JSON-line requests)."""
        peer = writer.get_extra_info("peername", "unknown")
        logger.debug("Client connected: %s", peer)
        if self._on_client_connect is not None:
            with contextlib.suppress(Exception):
                self._on_client_connect()
        try:
            while True:
                raw = await reader.readline()
                if not raw:
                    break  # Client disconnected

                if len(raw) > MAX_LINE_BYTES:
                    logger.warning("Oversized message from %s (%d bytes)", peer, len(raw))
                    break

                await self._process_line(raw, writer)
        except (ConnectionError, OSError):
            logger.debug("Client disconnected: %s", peer)
        finally:
            if self._on_client_disconnect is not None:
                with contextlib.suppress(Exception):
                    self._on_client_disconnect()
            try:
                writer.close()
                await writer.wait_closed()
            except (ConnectionError, OSError):
                pass

    async def _process_line(
        self,
        raw: bytes,
        writer: asyncio.StreamWriter,
    ) -> None:
        """Parse, authenticate, dispatch, and respond for one JSON line."""
        line = raw.decode("utf-8").strip()
        if not line:
            return

        try:
            data = json.loads(line)
        except json.JSONDecodeError:
            response = CoreResponse.failure(
                request_id="unknown",
                code="PARSE_ERROR",
                message="Invalid JSON",
            )
            await self._write_response(writer, response)
            return

        bearer = data.pop("bearer_token", None)
        if bearer != self._token:
            request_id = data.get("request_id", "unknown")
            response = CoreResponse.failure(
                request_id=request_id,
                code="AUTH_FAILED",
                message="Invalid or missing bearer token",
            )
            await self._write_response(writer, response)
            return

        try:
            request = CoreRequest.model_validate(data)
        except Exception as exc:
            request_id = data.get("request_id", "unknown")
            response = CoreResponse.failure(
                request_id=request_id,
                code="VALIDATION_ERROR",
                message=str(exc),
            )
            await self._write_response(writer, response)
            return

        try:
            response = await self._handler(request)
        except Exception as exc:
            logger.exception("Unhandled error in request handler")
            response = CoreResponse.failure(
                request_id=request.request_id,
                code="INTERNAL_ERROR",
                message=str(exc),
            )

        await self._write_response(writer, response)

    @staticmethod
    async def _write_response(
        writer: asyncio.StreamWriter,
        response: CoreResponse,
    ) -> None:
        """Serialise a response as a JSON line and flush it to the writer."""
        payload = response.model_dump_json() + "\n"
        writer.write(payload.encode("utf-8"))
        await writer.drain()


__all__ = ["IPCServer"]
