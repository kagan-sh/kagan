"""IPC transport implementations for Kagan core communication.

Provides Unix socket transport on POSIX and TCP loopback fallback on Windows.
``DefaultTransport`` is automatically set to the best choice for the current platform.
"""

from __future__ import annotations

import asyncio
import contextlib
import logging
import os
import platform
import secrets
import sys
from dataclasses import dataclass
from typing import TYPE_CHECKING

from kagan.core.paths import get_core_runtime_dir

if TYPE_CHECKING:
    from collections.abc import Callable, Coroutine
    from typing import Any

    ClientHandler = Callable[
        [asyncio.StreamReader, asyncio.StreamWriter],
        Coroutine[Any, Any, None],
    ]

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Shared types
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ServerHandle:
    """Handle returned after starting a transport server.

    Attributes:
        transport_type: Identifier string (``socket`` or ``tcp``).
        address: The connection address (file path or hostname).
        port: TCP port when applicable; ``None`` for socket transport.
        close: Async callable to shut down the server gracefully.
    """

    transport_type: str
    address: str
    port: int | None = None
    close: Callable[[], Coroutine[Any, Any, None]] | None = None


# ---------------------------------------------------------------------------
# Unix socket transport
# ---------------------------------------------------------------------------

_SOCKET_NAME = "core.sock"


def _default_socket_path() -> str:
    """Return the default path for the Unix domain socket."""
    return str(get_core_runtime_dir() / _SOCKET_NAME)


class UnixSocketTransport:
    """IPC transport over Unix domain sockets.

    Only available on macOS and Linux.  On Windows this class raises
    ``NotImplementedError`` at construction time.
    """

    def __init__(self, path: str | None = None) -> None:
        if platform.system() == "Windows":
            msg = "Unix sockets are not supported on Windows"
            raise NotImplementedError(msg)
        self._path = path or _default_socket_path()

    async def start_server(
        self,
        handler: ClientHandler,
    ) -> ServerHandle:
        """Bind a Unix socket server at the configured path.

        Any stale socket file is removed before binding.
        """
        with contextlib.suppress(FileNotFoundError):
            os.unlink(self._path)

        parent = os.path.dirname(self._path)
        if parent:
            os.makedirs(parent, exist_ok=True)

        server = await asyncio.start_unix_server(handler, path=self._path)

        if sys.platform != "win32":
            os.chmod(self._path, 0o600)

        logger.info("Unix socket server listening on %s", self._path)

        async def _close() -> None:
            server.close()
            await server.wait_closed()
            with contextlib.suppress(FileNotFoundError):
                os.unlink(self._path)
            logger.info("Unix socket server stopped")

        return ServerHandle(
            transport_type="socket",
            address=self._path,
            port=None,
            close=_close,
        )

    async def connect(
        self,
        address: str,
        port: int | None = None,
    ) -> tuple[asyncio.StreamReader, asyncio.StreamWriter]:
        """Open a connection to the Unix socket at *address*."""
        reader, writer = await asyncio.open_unix_connection(address)
        logger.debug("Connected to Unix socket at %s", address)
        return reader, writer


# ---------------------------------------------------------------------------
# TCP loopback transport
# ---------------------------------------------------------------------------

_LOCALHOST = "127.0.0.1"
_TOKEN_BYTES = 32


class TCPLoopbackTransport:
    """IPC transport over a TCP socket bound to localhost.

    Used as a cross-platform fallback when Unix sockets or named pipes are
    unavailable.  A random port is chosen by the OS, and a secret token is
    generated so that only authorised clients can complete the handshake.

    The handshake token is distinct from the per-request bearer token used
    by ``IPCServer`` -- it proves the client discovered the endpoint through
    a trusted channel (the endpoint file on disk).
    """

    def __init__(self, host: str | None = None) -> None:
        self._host = host or _LOCALHOST
        self._handshake_token: str | None = None

    def set_handshake_token(self, token: str) -> None:
        """Configure a deterministic handshake token before server start."""
        self._handshake_token = token

    @property
    def handshake_token(self) -> str | None:
        """The secret handshake token generated when the server starts."""
        return self._handshake_token

    async def start_server(
        self,
        handler: ClientHandler,
    ) -> ServerHandle:
        """Bind a TCP server on localhost with a random port.

        A fresh handshake token is generated and stored on the instance so
        the caller can persist it alongside the endpoint descriptor.
        """
        if self._handshake_token is None:
            self._handshake_token = secrets.token_hex(_TOKEN_BYTES)

        async def _handshake_wrapper(
            reader: asyncio.StreamReader,
            writer: asyncio.StreamWriter,
        ) -> None:
            """Validate the handshake token before delegating to *handler*."""
            try:
                raw = await asyncio.wait_for(reader.readline(), timeout=5.0)
                line = raw.decode("utf-8").strip()
                if line != self._handshake_token:
                    logger.warning("TCP handshake failed: invalid token")
                    writer.close()
                    await writer.wait_closed()
                    return
                writer.write(b"OK\n")
                await writer.drain()
            except (TimeoutError, ConnectionError, OSError):
                logger.warning("TCP handshake error", exc_info=True)
                writer.close()
                await writer.wait_closed()
                return

            await handler(reader, writer)

        server = await asyncio.start_server(
            _handshake_wrapper,
            host=self._host,
            port=0,  # OS picks a free port
        )

        addrs = server.sockets[0].getsockname() if server.sockets else (self._host, 0)
        bound_port: int = addrs[1]

        logger.info("TCP loopback server listening on %s:%d", self._host, bound_port)

        async def _close() -> None:
            server.close()
            await server.wait_closed()
            logger.info("TCP loopback server stopped")

        return ServerHandle(
            transport_type="tcp",
            address=self._host,
            port=bound_port,
            close=_close,
        )

    async def connect(
        self,
        address: str,
        port: int | None = None,
        *,
        handshake_token: str | None = None,
    ) -> tuple[asyncio.StreamReader, asyncio.StreamWriter]:
        """Open a TCP connection and complete the handshake.

        Args:
            address: Hostname (should be ``127.0.0.1``).
            port: TCP port advertised by the server.
            handshake_token: The secret token obtained from the endpoint file.

        Raises:
            ConnectionError: If the handshake is rejected or times out.
        """
        if port is None:
            msg = "TCP transport requires a port"
            raise ValueError(msg)
        if not handshake_token:
            msg = "TCP transport requires a handshake token"
            raise ValueError(msg)

        reader, writer = await asyncio.open_connection(address, port)
        writer.write((handshake_token + "\n").encode("utf-8"))
        await writer.drain()

        raw = await asyncio.wait_for(reader.readline(), timeout=5.0)
        ack = raw.decode("utf-8").strip()
        if ack != "OK":
            writer.close()
            await writer.wait_closed()
            msg = "TCP handshake rejected by server"
            raise ConnectionError(msg)

        logger.debug("Connected to TCP server at %s:%d", address, port)
        return reader, writer


# ---------------------------------------------------------------------------
# Default transport selection
# ---------------------------------------------------------------------------

if sys.platform == "win32":
    DefaultTransport = TCPLoopbackTransport
else:
    DefaultTransport = UnixSocketTransport

__all__ = [
    "ClientHandler",
    "DefaultTransport",
    "ServerHandle",
    "TCPLoopbackTransport",
    "UnixSocketTransport",
]
