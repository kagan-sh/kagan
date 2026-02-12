from __future__ import annotations

from unittest.mock import AsyncMock

from kagan.core.ipc.contracts import CoreErrorDetail, CoreResponse

SESSION = "test-session-1"


def make_client(response_data: dict | None = None, ok: bool = True) -> AsyncMock:
    """Create a mock IPCClient that returns a fixed CoreResponse."""
    client = AsyncMock()
    error = None if ok else CoreErrorDetail(code="ERR", message="fail")
    resp = CoreResponse(
        request_id="test-req",
        ok=ok,
        result=response_data,
        error=error,
    )
    client.request = AsyncMock(return_value=resp)
    return client
