"""Buffered shell reading utility."""

from __future__ import annotations

import asyncio
from contextlib import suppress
from time import monotonic


async def shell_read(
    reader: asyncio.StreamReader,
    buffer_size: int,
    buffer_period: float = 0.01,
    max_buffer_duration: float = 0.016,
) -> bytes:
    """Read with buffering to reduce chunk frequency.

    Args:
        reader: Stream reader instance.
        buffer_size: Maximum buffer size.
        buffer_period: Time in seconds where reads are batched.
        max_buffer_duration: Maximum time in seconds to buffer.

    Returns:
        Bytes read. May be empty on the last read.
    """
    try:
        data = await reader.read(buffer_size)
    except OSError:
        data = b""
    if data:
        buffer_time = monotonic() + max_buffer_duration
        with suppress(asyncio.TimeoutError):
            while len(data) < buffer_size and (time := monotonic()) < buffer_time:
                async with asyncio.timeout(min(buffer_time - time, buffer_period)):
                    try:
                        if chunk := await reader.read(buffer_size - len(data)):
                            data += chunk
                        else:
                            break
                    except OSError:
                        break
    return data
