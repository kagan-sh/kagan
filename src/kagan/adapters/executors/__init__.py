"""Executor adapter contracts."""

from kagan.adapters.executors.acp import ACP_CAPABILITIES, ACPExecutor, ACPExecutorConfig
from kagan.adapters.executors.base import (
    ExecutionOutput,
    ExecutionRequest,
    ExecutionResult,
    ExecutorAdapter,
    ExecutorCapabilities,
)
from kagan.adapters.executors.scripts import (
    SCRIPTS_CAPABILITIES,
    ScriptExecutor,
    ScriptExecutorConfig,
)
from kagan.adapters.executors.tmux import TMUX_CAPABILITIES, TmuxExecutor, TmuxExecutorConfig

__all__ = [
    "ACP_CAPABILITIES",
    "SCRIPTS_CAPABILITIES",
    "TMUX_CAPABILITIES",
    "ACPExecutor",
    "ACPExecutorConfig",
    "ExecutionOutput",
    "ExecutionRequest",
    "ExecutionResult",
    "ExecutorAdapter",
    "ExecutorCapabilities",
    "ScriptExecutor",
    "ScriptExecutorConfig",
    "TmuxExecutor",
    "TmuxExecutorConfig",
]
