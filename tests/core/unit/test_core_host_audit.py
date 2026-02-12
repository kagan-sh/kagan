from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING, cast
from unittest.mock import AsyncMock

from kagan.core.host import CoreHost
from kagan.core.ipc.contracts import CoreRequest, CoreResponse

if TYPE_CHECKING:
    from kagan.core.bootstrap import AppContext


async def test_record_audit_event_uses_nested_result_success_when_present() -> None:
    host = CoreHost()
    audit_repository = SimpleNamespace(record=AsyncMock())
    host._ctx = cast("AppContext", SimpleNamespace(audit_repository=audit_repository))

    request = CoreRequest(
        session_id="session-1",
        capability="jobs",
        method="submit",
        params={"task_id": "T1"},
    )
    response = CoreResponse.success(
        request.request_id,
        result={"success": False, "code": "UNSUPPORTED_ACTION"},
    )

    await host._record_audit_event(request, response)

    kwargs = audit_repository.record.await_args.kwargs
    assert kwargs["success"] is False


async def test_record_audit_event_defaults_to_transport_success_without_nested_flag() -> None:
    host = CoreHost()
    audit_repository = SimpleNamespace(record=AsyncMock())
    host._ctx = cast("AppContext", SimpleNamespace(audit_repository=audit_repository))

    request = CoreRequest(
        session_id="session-1",
        capability="tasks",
        method="list",
        params={},
    )
    response = CoreResponse.success(request.request_id, result={"count": 1})

    await host._record_audit_event(request, response)

    kwargs = audit_repository.record.await_args.kwargs
    assert kwargs["success"] is True
