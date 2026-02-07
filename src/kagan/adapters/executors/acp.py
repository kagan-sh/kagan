"""ACP executor contract and capabilities."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Protocol

from kagan.adapters.executors.base import ExecutorAdapter, ExecutorCapabilities

ACP_CAPABILITIES = ExecutorCapabilities(
    supports_sessions=True,
    supports_streaming=True,
    supports_interactive=False,
    supports_cancellation=True,
    supports_env=True,
    supports_workdir=True,
    supports_timeout=True,
)


@dataclass(frozen=True)
class ACPExecutorConfig:
    """Configuration for ACP executor backends."""

    run_command: list[str]
    interactive_command: list[str] | None = None
    model_env_var: str | None = None
    readonly: bool = False


class ACPExecutor(ExecutorAdapter, Protocol):
    """Executor protocol for ACP-backed agents."""

    config: ACPExecutorConfig
