from __future__ import annotations

import sys
import tempfile
from shutil import rmtree
from typing import TYPE_CHECKING

import pytest

from kagan.core.ipc.transports import TCPLoopbackTransport, UnixSocketTransport

if TYPE_CHECKING:
    import asyncio


@pytest.mark.asyncio
async def test_tcp_loopback_transport_contract_round_trip() -> None:
    """TCP transport should expose a valid server handle and round-trip payload."""
    transport = TCPLoopbackTransport()

    async def _handler(
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
    ) -> None:
        payload = await reader.readline()
        writer.write(payload)
        await writer.drain()
        writer.close()
        await writer.wait_closed()

    handle = await transport.start_server(_handler)
    assert handle.transport_type == "tcp"
    assert handle.address == "127.0.0.1"
    assert isinstance(handle.port, int)
    assert handle.port is not None
    assert handle.close is not None

    reader, writer = await transport.connect(
        handle.address,
        handle.port,
        handshake_token=transport.handshake_token,
    )
    writer.write(b"hello\n")
    await writer.drain()
    echoed = await reader.readline()
    assert echoed == b"hello\n"

    writer.close()
    await writer.wait_closed()
    await handle.close()


@pytest.mark.asyncio
async def test_tcp_loopback_transport_handles_large_line_payload() -> None:
    transport = TCPLoopbackTransport()
    payload = b"x" * (128 * 1024) + b"\n"

    async def _handler(
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
    ) -> None:
        echoed = await reader.readline()
        writer.write(echoed)
        await writer.drain()
        writer.close()
        await writer.wait_closed()

    handle = await transport.start_server(_handler)
    reader, writer = await transport.connect(
        handle.address,
        handle.port,
        handshake_token=transport.handshake_token,
    )
    writer.write(payload)
    await writer.drain()
    echoed = await reader.readline()
    assert echoed == payload

    writer.close()
    await writer.wait_closed()
    assert handle.close is not None
    await handle.close()


@pytest.mark.asyncio
@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
async def test_unix_socket_transport_contract() -> None:
    """Unix transport should expose socket handle metadata and allow local connect."""
    short_tmp = tempfile.mkdtemp(prefix="k-", dir="/tmp")
    socket_path = f"{short_tmp}/core.sock"
    transport = UnixSocketTransport(path=socket_path)

    try:

        async def _handler(
            reader: asyncio.StreamReader,
            writer: asyncio.StreamWriter,
        ) -> None:
            del reader
            writer.close()
            await writer.wait_closed()

        handle = await transport.start_server(_handler)
        assert handle.transport_type == "socket"
        assert handle.address == socket_path
        assert handle.port is None
        assert handle.close is not None

        reader, writer = await transport.connect(handle.address)
        del reader
        writer.close()
        await writer.wait_closed()
        await handle.close()
    finally:
        rmtree(short_tmp, ignore_errors=True)


@pytest.mark.asyncio
@pytest.mark.skipif(sys.platform == "win32", reason="Unix sockets unavailable on Windows")
async def test_unix_socket_transport_handles_large_line_payload() -> None:
    short_tmp = tempfile.mkdtemp(prefix="k-", dir="/tmp")
    socket_path = f"{short_tmp}/core.sock"
    transport = UnixSocketTransport(path=socket_path)
    payload = b"x" * (128 * 1024) + b"\n"

    try:

        async def _handler(
            reader: asyncio.StreamReader,
            writer: asyncio.StreamWriter,
        ) -> None:
            echoed = await reader.readline()
            writer.write(echoed)
            await writer.drain()
            writer.close()
            await writer.wait_closed()

        handle = await transport.start_server(_handler)
        reader, writer = await transport.connect(handle.address)
        writer.write(payload)
        await writer.drain()
        echoed = await reader.readline()
        assert echoed == payload

        writer.close()
        await writer.wait_closed()
        assert handle.close is not None
        await handle.close()
    finally:
        rmtree(short_tmp, ignore_errors=True)
