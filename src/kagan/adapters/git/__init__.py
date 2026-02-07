"""Git adapter contracts."""

from kagan.adapters.git.operations import GitOperationsAdapter
from kagan.adapters.git.worktrees import GitWorktreeAdapter

__all__ = [
    "GitOperationsAdapter",
    "GitWorktreeAdapter",
]
