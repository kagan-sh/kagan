"""Request-scoped context for core handler dispatch.

Core request handlers are invoked with only ``(api, params)``. In a few cases we
need access to the effective session binding (profile/origin/namespace) that was
resolved by :class:`kagan.core.host.CoreHost` at the IPC boundary.

This module provides a small contextvar-based bridge to expose that information
to downstream handler code without introducing client-controlled parameters.
"""

from __future__ import annotations

from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Iterator

    from kagan.core.ipc.contracts import CoreRequest
    from kagan.core.session_binding import SessionBinding


@dataclass(frozen=True, slots=True)
class RequestContext:
    request: CoreRequest
    binding: SessionBinding


_REQUEST_CONTEXT: ContextVar[RequestContext | None] = ContextVar(
    "kagan_request_context",
    default=None,
)


@contextmanager
def request_context(ctx: RequestContext) -> Iterator[None]:
    token = _REQUEST_CONTEXT.set(ctx)
    try:
        yield
    finally:
        _REQUEST_CONTEXT.reset(token)


def get_request_context() -> RequestContext | None:
    return _REQUEST_CONTEXT.get()


def require_request_context() -> RequestContext:
    ctx = get_request_context()
    if ctx is None:
        raise RuntimeError("Request context is not available")
    return ctx


__all__ = [
    "RequestContext",
    "get_request_context",
    "request_context",
    "require_request_context",
]
