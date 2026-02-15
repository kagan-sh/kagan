"""IPC request/response contract types for Kagan core communication."""

from __future__ import annotations

from typing import Any
from uuid import uuid4

from pydantic import BaseModel, Field


def _new_request_id() -> str:
    return uuid4().hex


class CoreRequest(BaseModel):
    """Envelope for a single IPC request sent from a client to the core.

    Each request targets a *capability* (logical service group) and a *method*
    within that capability.  ``params`` carries the method-specific payload.
    ``idempotency_key`` allows the core to de-duplicate retried requests.
    """

    request_id: str = Field(
        default_factory=_new_request_id,
        description="Unique identifier for this request",
    )
    session_id: str = Field(
        description="Identifier of the client session originating the request",
    )
    session_profile: str | None = Field(
        default=None,
        description=(
            "Capability profile for this session (viewer|planner|pair_worker|operator|maintainer)"
        ),
    )
    session_origin: str | None = Field(
        default=None,
        description=(
            "Origin lane for this session (legacy|kagan|kagan_admin|tui). "
            "Used for server-side capability ceilings and namespace constraints."
        ),
    )
    client_version: str | None = Field(
        default=None,
        description="Client-reported kagan package version for compatibility checks",
    )
    capability: str = Field(
        description="Logical service group (e.g. 'tasks', 'agents', 'config')",
    )
    method: str = Field(
        description="Method name within the capability (e.g. 'list', 'create')",
    )
    params: dict[str, Any] = Field(
        default_factory=dict,
        description="Method-specific parameters",
    )
    idempotency_key: str | None = Field(
        default=None,
        description="Optional key for request de-duplication on retries",
    )


class CoreErrorDetail(BaseModel):
    """Structured error information returned inside a ``CoreResponse``."""

    code: str = Field(
        description="Machine-readable error code (e.g. 'NOT_FOUND', 'TIMEOUT')",
    )
    message: str = Field(
        description="Human-readable error description",
    )


class CoreResponse(BaseModel):
    """Envelope for a single IPC response sent from the core to a client.

    ``ok`` is *True* when the request succeeded; ``result`` then carries the
    response payload.  When ``ok`` is *False*, ``error`` contains a structured
    error with a machine-readable code and a human-readable message.
    """

    request_id: str = Field(
        description="Echoed request_id from the originating CoreRequest",
    )
    ok: bool = Field(
        description="Whether the request was processed successfully",
    )
    result: dict[str, Any] | None = Field(
        default=None,
        description="Response payload on success (None on failure)",
    )
    error: CoreErrorDetail | None = Field(
        default=None,
        description="Structured error detail on failure (None on success)",
    )

    @staticmethod
    def success(
        request_id: str,
        result: dict[str, Any] | None = None,
    ) -> CoreResponse:
        """Create a successful response."""
        return CoreResponse(request_id=request_id, ok=True, result=result)

    @staticmethod
    def failure(
        request_id: str,
        *,
        code: str,
        message: str,
    ) -> CoreResponse:
        """Create a failure response with an error detail."""
        return CoreResponse(
            request_id=request_id,
            ok=False,
            error=CoreErrorDetail(code=code, message=message),
        )


__all__ = [
    "CoreErrorDetail",
    "CoreRequest",
    "CoreResponse",
]
