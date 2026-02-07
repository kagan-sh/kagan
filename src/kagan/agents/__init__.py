"""Planner, scheduler, and worktree utilities for Kagan."""

from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.agents.planner import build_planner_prompt, parse_proposed_plan
from kagan.agents.prompt import build_prompt
from kagan.agents.scheduler import Scheduler
from kagan.agents.signals import Signal, SignalResult, parse_signal
from kagan.agents.worktree import WorktreeError, WorktreeManager, slugify

if TYPE_CHECKING:
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
    "parse_proposed_plan",
    "parse_signal",
    "slugify",
]


def __getattr__(name: str) -> object:
    if name == "SessionManager":
        from kagan.sessions import SessionManager

        return SessionManager
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
