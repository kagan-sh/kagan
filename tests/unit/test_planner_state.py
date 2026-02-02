"""Unit tests for PlannerState memory bounds."""

from __future__ import annotations

from datetime import datetime

import pytest

from kagan.limits import MAX_ACCUMULATED_CHUNKS, MAX_CONVERSATION_HISTORY
from kagan.ui.screens.planner.state import ChatMessage, PlannerPhase, PlannerState

pytestmark = pytest.mark.unit


class TestPlannerStateBounds:
    """Tests for PlannerState memory bounds."""

    def test_transition_trims_conversation_history_when_exceeds_limit(self):
        """Conversation history should be trimmed during state transitions."""
        # Create state with history exceeding limit
        oversized_history = [
            ChatMessage(role="user", content=f"message {i}", timestamp=datetime.now())
            for i in range(MAX_CONVERSATION_HISTORY + 50)
        ]

        state = PlannerState(
            phase=PlannerPhase.PROCESSING,
            conversation_history=oversized_history,
        )

        # Trigger a transition
        new_state = state.transition("done")

        # History should be trimmed to limit
        assert len(new_state.conversation_history) <= MAX_CONVERSATION_HISTORY

    def test_transition_trims_accumulated_response_when_exceeds_limit(self):
        """Accumulated response should be trimmed during state transitions."""
        # Create state with accumulated_response exceeding limit
        oversized_response = [f"chunk {i}" for i in range(MAX_ACCUMULATED_CHUNKS + 100)]

        state = PlannerState(
            phase=PlannerPhase.PROCESSING,
            accumulated_response=oversized_response,
        )

        # Trigger a transition that doesn't go to IDLE (which clears accumulated_response)
        new_state = state.transition("plan_received")

        # Response should be trimmed to limit
        assert len(new_state.accumulated_response) <= MAX_ACCUMULATED_CHUNKS

    def test_transition_preserves_lists_under_limit(self):
        """Lists under the limit should not be affected."""
        history = [
            ChatMessage(role="user", content="test", timestamp=datetime.now()) for _ in range(10)
        ]
        accumulated = ["chunk" for _ in range(20)]

        state = PlannerState(
            phase=PlannerPhase.PROCESSING,
            conversation_history=history,
            accumulated_response=accumulated,
        )

        # Use plan_received to avoid IDLE clearing accumulated_response
        new_state = state.transition("plan_received")

        # History should be unchanged
        assert len(new_state.conversation_history) == 10
        # Accumulated should be unchanged
        assert len(new_state.accumulated_response) == 20

    def test_transition_to_idle_clears_accumulated_response(self):
        """Transition to IDLE should clear accumulated_response."""
        accumulated = ["chunk" for _ in range(50)]

        state = PlannerState(
            phase=PlannerPhase.PROCESSING,
            accumulated_response=accumulated,
        )

        new_state = state.transition("done")

        assert new_state.phase == PlannerPhase.IDLE
        assert len(new_state.accumulated_response) == 0

    def test_transition_keeps_recent_history_when_trimming(self):
        """When trimming, the most recent messages should be kept."""
        # Create history with identifiable messages
        oversized_history = [
            ChatMessage(role="user", content=f"message_{i}", timestamp=datetime.now())
            for i in range(MAX_CONVERSATION_HISTORY + 50)
        ]

        state = PlannerState(
            phase=PlannerPhase.PROCESSING,
            conversation_history=oversized_history,
        )

        new_state = state.transition("done")

        # The last message should be the most recent one (highest index from original)
        last_kept = new_state.conversation_history[-1]
        expected_index = MAX_CONVERSATION_HISTORY + 50 - 1
        assert last_kept.content == f"message_{expected_index}"
