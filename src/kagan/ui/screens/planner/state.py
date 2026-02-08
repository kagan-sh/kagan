"""State machine for PlannerScreen."""

from __future__ import annotations

from dataclasses import dataclass, field, replace
from enum import Enum, auto
from typing import TYPE_CHECKING

from kagan.core.models.enums import ChatRole  # noqa: TC001 (used at runtime in dataclass)
from kagan.limits import MAX_ACCUMULATED_CHUNKS, MAX_CONVERSATION_HISTORY

if TYPE_CHECKING:
    from datetime import datetime

    from acp.schema import PlanEntry

    from kagan.acp import Agent
    from kagan.adapters.db.schema import Task
    from kagan.agents.refiner import PromptRefiner


class PlannerPhase(Enum):
    """Phases of the planner screen lifecycle."""

    IDLE = auto()
    PROCESSING = auto()
    REFINING = auto()
    AWAITING_APPROVAL = auto()
    CREATING_TASKS = auto()


@dataclass
class NoteInfo:
    """A note to display in the conversation."""

    text: str
    classes: str = ""


@dataclass
class ChatMessage:
    """A message in the planner conversation history."""

    role: ChatRole
    content: str
    timestamp: datetime
    plan_tasks: list[Task] | None = None
    # Parsed todo entries for PlanDisplay restoration
    todos: list[PlanEntry] | list[dict[str, object]] | None = None
    # Notes to display after this message (e.g., "Created N tasks", errors)
    notes: list[NoteInfo] = field(default_factory=list)


@dataclass
class PlannerState:
    """Unified state for the planner screen.

    This dataclass holds all mutable state for the planner, replacing
    the 15+ instance variables that were scattered across PlannerScreen.
    """

    phase: PlannerPhase = PlannerPhase.IDLE
    agent_ready: bool = False
    has_pending_plan: bool = False
    thinking_shown: bool = False
    todos_displayed: bool = False
    has_output: bool = False

    accumulated_response: list[str] = field(default_factory=list)

    conversation_history: list[ChatMessage] = field(default_factory=list)

    pending_plan: list[Task] | None = None

    input_text: str = ""

    agent: Agent | None = None
    refiner: PromptRefiner | None = None

    def can_submit(self) -> bool:
        """Check if user can submit a prompt."""
        return self.phase == PlannerPhase.IDLE and self.agent_ready

    def can_refine(self) -> bool:
        """Check if user can refine the prompt."""
        return self.phase == PlannerPhase.IDLE and self.agent_ready

    def transition(self, event: str) -> PlannerState:
        """State machine transitions.

        Returns a new PlannerState with updated phase and related fields.
        This is an immutable-style update pattern.
        """
        transitions: dict[tuple[PlannerPhase, str], PlannerPhase] = {
            (PlannerPhase.IDLE, "submit"): PlannerPhase.PROCESSING,
            (PlannerPhase.IDLE, "refine"): PlannerPhase.REFINING,
            (PlannerPhase.PROCESSING, "plan_received"): PlannerPhase.AWAITING_APPROVAL,
            (PlannerPhase.PROCESSING, "done"): PlannerPhase.IDLE,
            (PlannerPhase.PROCESSING, "error"): PlannerPhase.IDLE,
            (PlannerPhase.REFINING, "done"): PlannerPhase.IDLE,
            (PlannerPhase.REFINING, "error"): PlannerPhase.IDLE,
            (PlannerPhase.AWAITING_APPROVAL, "approved"): PlannerPhase.CREATING_TASKS,
            (PlannerPhase.AWAITING_APPROVAL, "rejected"): PlannerPhase.IDLE,
            (PlannerPhase.AWAITING_APPROVAL, "edit"): PlannerPhase.AWAITING_APPROVAL,
            (PlannerPhase.CREATING_TASKS, "done"): PlannerPhase.IDLE,
            (PlannerPhase.CREATING_TASKS, "error"): PlannerPhase.IDLE,
        }

        new_phase = transitions.get((self.phase, event), self.phase)

        accumulated = self.accumulated_response
        if len(accumulated) > MAX_ACCUMULATED_CHUNKS:
            accumulated = accumulated[-MAX_ACCUMULATED_CHUNKS:]

        history = self.conversation_history
        if len(history) > MAX_CONVERSATION_HISTORY:
            history = history[-MAX_CONVERSATION_HISTORY:]

        new_state = replace(
            self,
            phase=new_phase,
            has_pending_plan=self.has_pending_plan if new_phase != PlannerPhase.IDLE else False,
            thinking_shown=self.thinking_shown if new_phase == self.phase else False,
            todos_displayed=self.todos_displayed if new_phase == self.phase else False,
            accumulated_response=accumulated,
            conversation_history=history,
        )

        if new_phase == PlannerPhase.IDLE and self.phase != PlannerPhase.IDLE:
            new_state.accumulated_response = []

        return new_state

    def with_agent_ready(self, ready: bool) -> PlannerState:
        """Return new state with agent_ready updated."""
        return replace(self, agent_ready=ready)

    def with_pending_plan(self, plan: list[Task] | None) -> PlannerState:
        """Return new state with pending_plan updated."""
        return replace(self, has_pending_plan=plan is not None, pending_plan=plan)


@dataclass
class PersistentPlannerState:
    """State preserved across screen switches.

    This is a subset of PlannerState that should be persisted
    when the user navigates away from the planner screen.
    """

    conversation_history: list[ChatMessage]
    pending_plan: list[Task] | None
    input_text: str
    active_repo_id: str | None
    project_root: str
    agent: Agent | None = None
    refiner: PromptRefiner | None = None
    is_running: bool = False
    agent_ready: bool = False
