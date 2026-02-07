"""Script executor contract and capabilities."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Protocol

from kagan.adapters.executors.base import ExecutorAdapter, ExecutorCapabilities

SCRIPTS_CAPABILITIES = ExecutorCapabilities(
    supports_sessions=False,
    supports_streaming=True,
    supports_interactive=False,
    supports_cancellation=True,
    supports_env=True,
    supports_workdir=True,
    supports_timeout=True,
)


@dataclass(frozen=True)
class ScriptExecutorConfig:
    """Configuration for script-based execution."""

    default_shell: str | None = None


class ScriptExecutor(ExecutorAdapter, Protocol):
    """Executor protocol for script execution."""

    config: ScriptExecutorConfig
