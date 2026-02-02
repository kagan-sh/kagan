"""State machine for PlannerScreen."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum, auto
from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from datetime import datetime

    from kagan.acp.agent import Agent
    from kagan.agents.refiner import PromptRefiner
    from kagan.database.models import Ticket


class PlannerPhase(Enum):
    """Phases of the planner screen lifecycle."""

    IDLE = auto()
    PROCESSING = auto()
    REFINING = auto()
    AWAITING_APPROVAL = auto()
    CREATING_TICKETS = auto()


@dataclass
class NoteInfo:
    """A note to display in the conversation."""

    text: str
    classes: str = ""


@dataclass
class ChatMessage:
    """A message in the planner conversation history."""

    role: Literal["user", "assistant"]
    content: str
    timestamp: datetime
    plan_tickets: list[Ticket] | None = None
    # Parsed todo entries for PlanDisplay restoration
    todos: list[dict] | None = None
    # Notes to display after this message (e.g., "Created N tickets", errors)
    notes: list[NoteInfo] = field(default_factory=list)


@dataclass
class SlashCommand:
    """A slash command available in the planner."""

    command: str
    help: str


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

    # Accumulated response text from agent
    accumulated_response: list[str] = field(default_factory=list)

    # Conversation history for context injection
    conversation_history: list[ChatMessage] = field(default_factory=list)

    # Pending plan tickets awaiting approval
    pending_plan: list[Ticket] | None = None

    # Input text to preserve across screen switches
    input_text: str = ""

    # Agent and refiner kept alive across screen switches
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
            (PlannerPhase.AWAITING_APPROVAL, "approved"): PlannerPhase.CREATING_TICKETS,
            (PlannerPhase.AWAITING_APPROVAL, "rejected"): PlannerPhase.IDLE,
            (PlannerPhase.AWAITING_APPROVAL, "edit"): PlannerPhase.AWAITING_APPROVAL,
            (PlannerPhase.CREATING_TICKETS, "done"): PlannerPhase.IDLE,
            (PlannerPhase.CREATING_TICKETS, "error"): PlannerPhase.IDLE,
        }

        new_phase = transitions.get((self.phase, event), self.phase)

        # Build updated state based on transition
        new_state = PlannerState(
            phase=new_phase,
            agent_ready=self.agent_ready,
            has_pending_plan=self.has_pending_plan if new_phase != PlannerPhase.IDLE else False,
            thinking_shown=self.thinking_shown if new_phase == self.phase else False,
            todos_displayed=self.todos_displayed if new_phase == self.phase else False,
            has_output=self.has_output,
            accumulated_response=self.accumulated_response,
            conversation_history=self.conversation_history,
            pending_plan=self.pending_plan,
            input_text=self.input_text,
            agent=self.agent,
            refiner=self.refiner,
        )

        # Reset accumulated response on transition to IDLE
        if new_phase == PlannerPhase.IDLE and self.phase != PlannerPhase.IDLE:
            new_state.accumulated_response = []

        return new_state

    def with_agent_ready(self, ready: bool) -> PlannerState:
        """Return new state with agent_ready updated."""
        return PlannerState(
            phase=self.phase,
            agent_ready=ready,
            has_pending_plan=self.has_pending_plan,
            thinking_shown=self.thinking_shown,
            todos_displayed=self.todos_displayed,
            has_output=self.has_output,
            accumulated_response=self.accumulated_response,
            conversation_history=self.conversation_history,
            pending_plan=self.pending_plan,
            input_text=self.input_text,
            agent=self.agent,
            refiner=self.refiner,
        )

    def with_pending_plan(self, plan: list[Ticket] | None) -> PlannerState:
        """Return new state with pending_plan updated."""
        return PlannerState(
            phase=self.phase,
            agent_ready=self.agent_ready,
            has_pending_plan=plan is not None,
            thinking_shown=self.thinking_shown,
            todos_displayed=self.todos_displayed,
            has_output=self.has_output,
            accumulated_response=self.accumulated_response,
            conversation_history=self.conversation_history,
            pending_plan=plan,
            input_text=self.input_text,
            agent=self.agent,
            refiner=self.refiner,
        )


@dataclass
class PersistentPlannerState:
    """State preserved across screen switches.

    This is a subset of PlannerState that should be persisted
    when the user navigates away from the planner screen.
    """

    conversation_history: list[ChatMessage]
    pending_plan: list[Ticket] | None
    input_text: str
    agent: Agent | None = None
    refiner: PromptRefiner | None = None
    is_running: bool = False
    agent_ready: bool = False
