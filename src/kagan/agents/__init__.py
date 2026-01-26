"""Agent management for Kagan."""

from kagan.acp.agent import Agent
from kagan.acp.messages import AgentFail, AgentReady
from kagan.agents.manager import AgentManager
from kagan.agents.scheduler import Scheduler
from kagan.agents.worktree import WorktreeError, WorktreeManager, slugify

__all__ = [
    "Agent",
    "AgentFail",
    "AgentManager",
    "AgentReady",
    "Scheduler",
    "WorktreeError",
    "WorktreeManager",
    "slugify",
]
