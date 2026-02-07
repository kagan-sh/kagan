"""Tmux executor contract and capabilities."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Protocol

from kagan.adapters.executors.base import ExecutorAdapter, ExecutorCapabilities

TMUX_CAPABILITIES = ExecutorCapabilities(
    supports_sessions=True,
    supports_streaming=False,
    supports_interactive=True,
    supports_cancellation=True,
    supports_env=True,
    supports_workdir=True,
    supports_timeout=False,
)


@dataclass(frozen=True)
class TmuxExecutorConfig:
    """Configuration for tmux-backed execution."""

    session_prefix: str = "kagan"
    shell: str | None = None


class TmuxExecutor(ExecutorAdapter, Protocol):
    """Executor protocol for tmux sessions."""

    config: TmuxExecutorConfig
