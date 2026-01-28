"""Planner, scheduler, and worktree utilities for Kagan."""

from kagan.agents.planner import build_planner_prompt, parse_plan
from kagan.agents.prompt import build_prompt
from kagan.agents.scheduler import Scheduler
from kagan.agents.signals import Signal, SignalResult, parse_signal
from kagan.agents.worktree import WorktreeError, WorktreeManager, slugify
from kagan.sessions import SessionManager

__all__ = [
    "Scheduler",
    "SessionManager",
    "Signal",
    "SignalResult",
    "WorktreeError",
    "WorktreeManager",
    "build_planner_prompt",
    "build_prompt",
    "parse_plan",
    "parse_signal",
    "slugify",
]
