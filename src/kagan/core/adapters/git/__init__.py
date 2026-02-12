"""Git adapter contracts."""

from kagan.core.adapters.git.operations import (
    GitAdapterBase,
    GitCommandResult,
    GitCommandRunner,
    GitOperationsAdapter,
    GitOperationsProtocol,
    has_tracked_uncommitted_changes,
)
from kagan.core.adapters.git.worktrees import GitWorktreeAdapter, GitWorktreeProtocol

__all__ = [
    "GitAdapterBase",
    "GitCommandResult",
    "GitCommandRunner",
    "GitOperationsAdapter",
    "GitOperationsProtocol",
    "GitWorktreeAdapter",
    "GitWorktreeProtocol",
    "has_tracked_uncommitted_changes",
]
