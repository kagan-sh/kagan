"""Review modal UI state helpers."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.css.query import NoMatches
from textual.widgets import Button, Static

from kagan.core.models.enums import StreamPhase
from kagan.ui.modals.review_prompt import extract_review_decision
from kagan.ui.utils.animation import WAVE_FRAMES, WAVE_INTERVAL_MS

if TYPE_CHECKING:
    from textual.timer import Timer

    from kagan.ui.modals.review import ReviewModal


class ReviewStateMixin:
    """Phase, decision and animation helpers."""

    _anim_timer: Timer | None

    def _set_phase(self: ReviewModal, phase: StreamPhase) -> None:
        """Update phase and UI state."""
        self._phase = phase
        badge = self.query_one("#phase-badge", Static)
        badge.update(f"{phase.icon} {phase.label}")
        badge.set_classes(f"phase-badge phase-{phase.value}")

        gen_btn = self.query_one("#generate-btn", Button)
        cancel_btn = self.query_one("#cancel-btn", Button)
        if self._read_only:
            gen_btn.add_class("hidden")
            cancel_btn.add_class("hidden")
            self._sync_review_action_state()
            return

        if phase == StreamPhase.IDLE:
            self._stop_animation()
            gen_btn.label = "Review (g)"
            gen_btn.variant = "primary"
            gen_btn.remove_class("hidden")
            gen_btn.disabled = False
            cancel_btn.add_class("hidden")
        elif phase in (StreamPhase.THINKING, StreamPhase.STREAMING):
            self._start_animation()
            gen_btn.add_class("hidden")
            cancel_btn.remove_class("hidden")
        else:
            self._stop_animation()
            gen_btn.label = "Regenerate (g)"
            gen_btn.variant = "default"
            gen_btn.remove_class("hidden")
            gen_btn.disabled = False
            cancel_btn.add_class("hidden")
        self._sync_review_action_state()

    def _set_decision(self: ReviewModal, decision: str | None) -> None:
        badge = self.query_one("#decision-badge", Static)
        if decision == "approved":
            badge.update("Decision: Approve")
            badge.set_classes("decision-badge decision-approved")
            return
        if decision == "rejected":
            badge.update("Decision: Reject")
            badge.set_classes("decision-badge decision-rejected")
            return
        badge.update("Decision: Pending")
        badge.set_classes("decision-badge decision-pending")

    def _sync_decision_from_output(self: ReviewModal) -> None:
        output = self._get_chat_panel().output
        decision = extract_review_decision(output.get_text_content())
        self._set_decision(decision)

    def _sync_review_action_state(self: ReviewModal) -> None:
        if self._read_only:
            return
        try:
            approve_btn = self.query_one("#approve-btn", Button)
        except NoMatches:
            return
        queue_pending = self._review_queue_pending
        review_running = self._phase in (StreamPhase.THINKING, StreamPhase.STREAMING)
        approve_btn.disabled = queue_pending or review_running
        if queue_pending:
            approve_btn.tooltip = "Process queued review messages before approval."
        elif review_running:
            approve_btn.tooltip = "Wait for review to complete before approval."
        else:
            approve_btn.tooltip = ""

    def _start_animation(self: ReviewModal) -> None:
        """Start wave animation for thinking/streaming state."""
        if self._anim_timer is None:
            self._anim_frame = 0
            self._anim_timer = self.set_interval(WAVE_INTERVAL_MS / 1000, self._next_frame)

    def _stop_animation(self: ReviewModal) -> None:
        """Stop wave animation."""
        if self._anim_timer is not None:
            self._anim_timer.stop()
            self._anim_timer = None

    def _next_frame(self: ReviewModal) -> None:
        """Advance to next animation frame."""
        self._anim_frame = (self._anim_frame + 1) % len(WAVE_FRAMES)
        badge = self.query_one("#phase-badge", Static)
        badge.update(f"{WAVE_FRAMES[self._anim_frame]} {self._phase.label}")
