"""Agent management for Kagan."""

from kagan.agents.manager import AgentManager
from kagan.agents.process import AgentProcess, AgentState
from kagan.agents.scheduler import Scheduler
from kagan.agents.shell_read import shell_read
from kagan.agents.worktree import WorktreeError, WorktreeManager, slugify

__all__ = [
    "AgentManager",
    "AgentProcess",
    "AgentState",
    "Scheduler",
    "WorktreeError",
    "WorktreeManager",
    "shell_read",
    "slugify",
]
