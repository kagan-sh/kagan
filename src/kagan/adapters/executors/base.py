"""Executor adapter contracts."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import TYPE_CHECKING, Any, Protocol

if TYPE_CHECKING:
    from collections.abc import AsyncIterator
    from pathlib import Path


@dataclass(frozen=True)
class ExecutorCapabilities:
    """Capability flags for an executor backend."""

    supports_sessions: bool
    supports_streaming: bool
    supports_interactive: bool
    supports_cancellation: bool
    supports_env: bool
    supports_workdir: bool
    supports_timeout: bool


@dataclass(frozen=True)
class ExecutionRequest:
    """Request for an executor backend."""

    execution_id: str
    command: list[str]
    workdir: Path | None = None
    env: dict[str, str] | None = None
    interactive: bool = False
    timeout_seconds: float | None = None
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class ExecutionOutput:
    """Streaming output from an execution."""

    stream: str
    chunk: str
    sequence: int
    emitted_at: datetime = field(default_factory=datetime.now)


@dataclass(frozen=True)
class ExecutionResult:
    """Final result for an execution."""

    execution_id: str
    exit_code: int
    duration_ms: int
    error: str | None = None
    metadata: dict[str, Any] = field(default_factory=dict)


class ExecutorAdapter(Protocol):
    """Adapter contract for execution backends."""

    name: str
    capabilities: ExecutorCapabilities

    async def execute(self, request: ExecutionRequest) -> ExecutionResult:
        """Run a command and return the execution result."""
        ...

    def stream(self, request: ExecutionRequest) -> AsyncIterator[ExecutionOutput]:
        """Stream output for long-running executions."""
        ...

    async def cancel(self, execution_id: str, *, reason: str | None = None) -> None:
        """Cancel an in-flight execution."""
        ...
