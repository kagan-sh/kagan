"""Shared IPC framing constants."""

from __future__ import annotations

MAX_LINE_BYTES = 4 * 1024 * 1024  # 4 MiB per JSON line (without framing overhead)
STREAM_LIMIT_BYTES = MAX_LINE_BYTES + 1  # Include trailing newline separator.

__all__ = ["MAX_LINE_BYTES", "STREAM_LIMIT_BYTES"]
