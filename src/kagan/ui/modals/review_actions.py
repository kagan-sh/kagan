"""User actions and key handlers for the review modal."""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

from textual.widgets import Static, TabbedContent

from kagan.core.models.enums import StreamPhase, TaskType
from kagan.ui.utils.clipboard import copy_with_notification

if TYPE_CHECKING:
    from kagan.app import KaganApp
    from kagan.ui.modals.review import ReviewModal


class ReviewActionsMixin:
    """Button handlers and user-facing actions."""

    def action_show_summary(self: ReviewModal) -> None:
        self._set_active_tab("review-summary")

    def action_show_diff(self: ReviewModal) -> None:
        self._set_active_tab("review-diff")

    def action_show_ai_review(self: ReviewModal) -> None:
        self._set_active_tab("review-ai")

    def action_show_agent_output(self: ReviewModal) -> None:
        self._set_active_tab("review-agent-output")

    def _set_active_tab(self: ReviewModal, tab_id: str) -> None:
        tabs = self.query_one("#review-tabs", TabbedContent)
        tabs.active = tab_id

    async def action_attach_session(self: ReviewModal) -> None:
        if self._task_model.task_type != TaskType.PAIR:
            return
        app = cast("KaganApp", self.app)
        if not await app.ctx.session_service.session_exists(self._task_model.id):
            self.notify("No active session for this task", severity="warning")
            return
        with self.app.suspend():
            await app.ctx.session_service.attach_session(self._task_model.id)

    async def action_generate_review(self: ReviewModal) -> None:
        """Generate or regenerate AI review."""
        from kagan.debug_log import log

        if self._read_only:
            self.notify("Read-only history view", severity="warning")
            return
        if self._live_review_attached:
            self.notify("Review is already running", severity="information")
            return

        log.info(f"[ReviewModal] Starting AI review (phase={self._phase})")

        if self._phase == StreamPhase.COMPLETE:
            await self.action_regenerate_review()
            return
        if self._phase != StreamPhase.IDLE:
            return

        self._set_decision(None)
        self._set_phase(StreamPhase.THINKING)
        chat_panel = self._get_chat_panel()
        chat_panel.remove_class("hidden")
        output = chat_panel.output
        await self._generate_ai_review(output)

    async def action_regenerate_review(self: ReviewModal) -> None:
        """Regenerate AI review."""
        if self._phase != StreamPhase.COMPLETE:
            return

        if self._agent:
            await self._agent.stop()
            self._agent = None

        output = self._get_chat_panel().output
        await output.clear()
        self._set_decision(None)
        self._set_phase(StreamPhase.THINKING)
        await self._generate_ai_review(output)

    async def action_cancel_review(self: ReviewModal) -> None:
        """Cancel ongoing review."""
        if self._live_review_attached and self._agent is None:
            self.notify("Review is managed by automation", severity="warning")
            return
        if self._phase not in (StreamPhase.THINKING, StreamPhase.STREAMING):
            return

        if self._prompt_task and not self._prompt_task.done():
            self._prompt_task.cancel()
        if self._agent:
            await self._agent.stop()
            self._agent = None

        output = self._get_chat_panel().output
        await output.post_note("Review cancelled", classes="dismissed")
        self._set_phase(StreamPhase.IDLE)

    async def action_view_diff(self: ReviewModal) -> None:
        """Open the diff modal for the current task."""
        await self._open_diff_modal()

    async def action_rebase(self: ReviewModal) -> None:
        """Rebase the task branch onto the base branch."""
        if self._read_only:
            self.notify("Read-only history view", severity="warning")
            return
        self.notify("Rebasing...", severity="information")
        success, message, conflict_files = await self._worktree.rebase_onto_base(
            self._task_model.id, self._base_branch
        )
        if success:
            self._diff_text = await self._worktree.get_diff(self._task_model.id, self._base_branch)
            self._render_diff_text(self._diff_text)
            diff_stats = await self._worktree.get_diff_stats(self._task_model.id, self._base_branch)
            self.query_one("#diff-stats", Static).update(diff_stats or "[dim](No changes)[/dim]")
            self.notify("Rebase successful", severity="information")
        elif conflict_files:
            self.dismiss("rebase_conflict")
        else:
            self.notify(f"Rebase failed: {message}", severity="error")

    def action_approve(self: ReviewModal) -> None:
        """Approve the review."""
        if self._read_only:
            self.notify("Read-only history view", severity="warning")
            return
        if self._phase in (StreamPhase.THINKING, StreamPhase.STREAMING):
            self.notify("Wait for review to complete before approval", severity="warning")
            return
        if self._review_queue_pending:
            self.notify("Process queued review messages before approval", severity="warning")
            return
        if self._no_changes:
            self.dismiss("exploratory")
        else:
            self.dismiss("approve")

    def action_reject(self: ReviewModal) -> None:
        """Reject the review."""
        if self._read_only:
            self.notify("Read-only history view", severity="warning")
            return
        self.dismiss("reject")

    async def action_close_or_cancel(self: ReviewModal) -> None:
        """Cancel review if in progress, otherwise close."""
        if self._phase in (StreamPhase.THINKING, StreamPhase.STREAMING):
            await self.action_cancel_review()
        else:
            self.dismiss(None)

    def action_copy(self: ReviewModal) -> None:
        """Copy review content to clipboard."""
        output = self._get_chat_panel().output
        review_text = output._agent_response._markdown if output._agent_response else ""

        content_parts = [f"# Review: {self._task_model.title}"]
        if self._diff_stats:
            content_parts.append(f"\n## Changes\n{self._diff_stats}")
        if review_text:
            content_parts.append(f"\n## AI Review\n{review_text}")

        copy_with_notification(self.app, "\n".join(content_parts), "Review")
