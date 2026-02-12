"""IPC contracts, endpoint discovery, and transport layer for Kagan core communication."""

from __future__ import annotations

from kagan.core.ipc.client import IPCClient
from kagan.core.ipc.contracts import CoreErrorDetail, CoreRequest, CoreResponse
from kagan.core.ipc.discovery import CoreEndpoint, discover_core_endpoint
from kagan.core.ipc.server import IPCServer

__all__ = [
    "CoreEndpoint",
    "CoreErrorDetail",
    "CoreRequest",
    "CoreResponse",
    "IPCClient",
    "IPCServer",
    "discover_core_endpoint",
]
