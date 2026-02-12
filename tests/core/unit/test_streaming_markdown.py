"""Tests for StreamingMarkdown stream lifecycle behavior."""

import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest

from kagan.core.models.enums import StreamRole
from kagan.tui.ui.widgets.streaming_markdown import StreamingMarkdown


@pytest.mark.asyncio
async def test_stop_stream_clears_and_stops_active_stream() -> None:
    widget = StreamingMarkdown(role=StreamRole.RESPONSE)
    stop = AsyncMock()
    widget._stream = SimpleNamespace(stop=stop)  # type: ignore[assignment]

    await widget.stop_stream()

    stop.assert_awaited_once()
    assert widget._stream is None


@pytest.mark.asyncio
async def test_clear_schedules_active_stream_stop() -> None:
    widget = StreamingMarkdown(role=StreamRole.RESPONSE)
    stop = AsyncMock()
    widget._stream = SimpleNamespace(stop=stop)  # type: ignore[assignment]
    widget.update = lambda *_args, **_kwargs: None  # type: ignore[method-assign]

    widget.clear()
    await asyncio.sleep(0)

    stop.assert_awaited_once()
    assert widget._stream is None
