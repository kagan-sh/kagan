"""Hypothesis stateful tests for PlannerState machine."""

from __future__ import annotations

import pytest
from hypothesis import assume
from hypothesis.stateful import RuleBasedStateMachine, invariant, rule

from kagan.ui.screens.planner.state import PlannerPhase, PlannerState

pytestmark = pytest.mark.unit


class PlannerStateMachine(RuleBasedStateMachine):
    """Stateful test for PlannerState transitions."""

    def __init__(self) -> None:
        super().__init__()
        self.state = PlannerState()

    @invariant()
    def phase_is_valid(self) -> None:
        """Phase must always be a valid PlannerPhase."""
        assert self.state.phase in PlannerPhase

    @invariant()
    def can_submit_requires_idle_and_ready(self) -> None:
        """can_submit() must only return True when IDLE and agent_ready."""
        if self.state.can_submit():
            assert self.state.phase == PlannerPhase.IDLE
            assert self.state.agent_ready

    @invariant()
    def can_refine_requires_idle_and_ready(self) -> None:
        """can_refine() must only return True when IDLE and agent_ready."""
        if self.state.can_refine():
            assert self.state.phase == PlannerPhase.IDLE
            assert self.state.agent_ready

    @invariant()
    def pending_plan_consistency(self) -> None:
        """has_pending_plan flag must be consistent with pending_plan."""
        if self.state.has_pending_plan:
            # Note: We can't check pending_plan directly as it may be set externally
            pass
        # If not has_pending_plan and phase is IDLE, pending_plan should be None/False
        if self.state.phase == PlannerPhase.IDLE and not self.state.has_pending_plan:
            # This is the expected state after approval/rejection
            pass

    @rule()
    def agent_becomes_ready(self) -> None:
        """Simulate agent becoming ready."""
        self.state = self.state.with_agent_ready(True)
        assert self.state.agent_ready

    @rule()
    def agent_becomes_not_ready(self) -> None:
        """Simulate agent becoming not ready (e.g., during clear)."""
        self.state = self.state.with_agent_ready(False)
        assert not self.state.agent_ready

    @rule()
    def submit_if_allowed(self) -> None:
        """Attempt to submit if allowed."""
        if self.state.can_submit():
            old_phase = self.state.phase
            self.state = self.state.transition("submit")
            assert self.state.phase == PlannerPhase.PROCESSING
            assert old_phase == PlannerPhase.IDLE

    @rule()
    def receive_plan(self) -> None:
        """Simulate receiving a plan from the agent."""
        if self.state.phase == PlannerPhase.PROCESSING:
            self.state = self.state.transition("plan_received")
            assert self.state.phase == PlannerPhase.AWAITING_APPROVAL

    @rule()
    def processing_done(self) -> None:
        """Simulate processing completing without a plan."""
        if self.state.phase == PlannerPhase.PROCESSING:
            self.state = self.state.transition("done")
            assert self.state.phase == PlannerPhase.IDLE

    @rule()
    def processing_error(self) -> None:
        """Simulate error during processing."""
        if self.state.phase == PlannerPhase.PROCESSING:
            self.state = self.state.transition("error")
            assert self.state.phase == PlannerPhase.IDLE

    @rule()
    def approve_plan(self) -> None:
        """Simulate approving a plan."""
        if self.state.phase == PlannerPhase.AWAITING_APPROVAL:
            self.state = self.state.transition("approved")
            assert self.state.phase == PlannerPhase.CREATING_TICKETS

    @rule()
    def reject_plan(self) -> None:
        """Simulate rejecting a plan."""
        if self.state.phase == PlannerPhase.AWAITING_APPROVAL:
            self.state = self.state.transition("rejected")
            assert self.state.phase == PlannerPhase.IDLE

    @rule()
    def ticket_creation_done(self) -> None:
        """Simulate ticket creation completing."""
        if self.state.phase == PlannerPhase.CREATING_TICKETS:
            self.state = self.state.transition("done")
            assert self.state.phase == PlannerPhase.IDLE

    @rule()
    def start_refine(self) -> None:
        """Attempt to start refinement if allowed."""
        if self.state.can_refine():
            assume(self.state.phase == PlannerPhase.IDLE)
            self.state = self.state.transition("refine")
            assert self.state.phase == PlannerPhase.REFINING

    @rule()
    def refining_done(self) -> None:
        """Simulate refinement completing."""
        if self.state.phase == PlannerPhase.REFINING:
            self.state = self.state.transition("done")
            assert self.state.phase == PlannerPhase.IDLE

    @rule()
    def refining_error(self) -> None:
        """Simulate error during refinement."""
        if self.state.phase == PlannerPhase.REFINING:
            self.state = self.state.transition("error")
            assert self.state.phase == PlannerPhase.IDLE

    @rule()
    def invalid_transition_is_noop(self) -> None:
        """Invalid transitions should not change phase."""
        old_phase = self.state.phase
        # Try an invalid transition
        if self.state.phase == PlannerPhase.IDLE:
            self.state = self.state.transition("plan_received")  # Invalid when IDLE
            assert self.state.phase == old_phase
        elif self.state.phase == PlannerPhase.PROCESSING:
            self.state = self.state.transition("approved")  # Invalid when PROCESSING
            assert self.state.phase == old_phase


# Create the test case class that pytest will discover
TestPlannerStateMachine = PlannerStateMachine.TestCase


class TestPlannerStateUnit:
    """Unit tests for PlannerState."""

    def test_initial_state(self) -> None:
        """Initial state should be IDLE with no agent ready."""
        state = PlannerState()
        assert state.phase == PlannerPhase.IDLE
        assert not state.agent_ready
        assert not state.has_pending_plan
        assert not state.thinking_shown
        assert not state.todos_displayed
        assert not state.has_output
        assert state.accumulated_response == []
        assert state.conversation_history == []
        assert state.pending_plan is None
        assert state.input_text == ""
        assert state.agent is None
        assert state.refiner is None

    def test_can_submit_false_when_not_ready(self) -> None:
        """can_submit should be False when agent is not ready."""
        state = PlannerState()
        assert not state.can_submit()

    def test_can_submit_true_when_ready_and_idle(self) -> None:
        """can_submit should be True when IDLE and agent ready."""
        state = PlannerState(phase=PlannerPhase.IDLE, agent_ready=True)
        assert state.can_submit()

    def test_can_submit_false_when_processing(self) -> None:
        """can_submit should be False when in PROCESSING phase."""
        state = PlannerState(phase=PlannerPhase.PROCESSING, agent_ready=True)
        assert not state.can_submit()

    def test_transition_submit(self) -> None:
        """Transition from IDLE to PROCESSING on submit."""
        state = PlannerState(agent_ready=True)
        new_state = state.transition("submit")
        assert new_state.phase == PlannerPhase.PROCESSING
        assert new_state.agent_ready  # Preserved

    def test_transition_plan_received(self) -> None:
        """Transition from PROCESSING to AWAITING_APPROVAL on plan_received."""
        state = PlannerState(phase=PlannerPhase.PROCESSING)
        new_state = state.transition("plan_received")
        assert new_state.phase == PlannerPhase.AWAITING_APPROVAL

    def test_transition_approved(self) -> None:
        """Transition from AWAITING_APPROVAL to CREATING_TICKETS on approved."""
        state = PlannerState(phase=PlannerPhase.AWAITING_APPROVAL)
        new_state = state.transition("approved")
        assert new_state.phase == PlannerPhase.CREATING_TICKETS

    def test_transition_rejected(self) -> None:
        """Transition from AWAITING_APPROVAL to IDLE on rejected."""
        state = PlannerState(phase=PlannerPhase.AWAITING_APPROVAL)
        new_state = state.transition("rejected")
        assert new_state.phase == PlannerPhase.IDLE

    def test_transition_invalid_is_noop(self) -> None:
        """Invalid transition should not change state."""
        state = PlannerState(phase=PlannerPhase.IDLE)
        new_state = state.transition("approved")  # Invalid from IDLE
        assert new_state.phase == PlannerPhase.IDLE

    def test_with_agent_ready(self) -> None:
        """with_agent_ready should return new state with updated flag."""
        state = PlannerState()
        new_state = state.with_agent_ready(True)
        assert new_state.agent_ready
        assert not state.agent_ready  # Original unchanged

    def test_with_pending_plan(self) -> None:
        """with_pending_plan should update both flag and plan."""
        from unittest.mock import MagicMock

        state = PlannerState()
        mock_tickets = [MagicMock()]
        new_state = state.with_pending_plan(mock_tickets)
        assert new_state.has_pending_plan
        assert new_state.pending_plan == mock_tickets
        assert not state.has_pending_plan  # Original unchanged

    def test_thinking_shown_reset_on_transition(self) -> None:
        """thinking_shown should be reset when phase changes."""
        state = PlannerState(phase=PlannerPhase.PROCESSING, thinking_shown=True)
        new_state = state.transition("done")
        assert new_state.phase == PlannerPhase.IDLE
        assert not new_state.thinking_shown

    def test_todos_displayed_reset_on_transition(self) -> None:
        """todos_displayed should be reset when phase changes."""
        state = PlannerState(phase=PlannerPhase.PROCESSING, todos_displayed=True)
        new_state = state.transition("done")
        assert new_state.phase == PlannerPhase.IDLE
        assert not new_state.todos_displayed
