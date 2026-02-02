"""Modal for reviewing ticket changes."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Literal

from textual import on
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Label, RichLog, Rule, Static

from kagan.acp import messages
from kagan.acp.agent import Agent
from kagan.constants import DIFF_MAX_LENGTH, MODAL_TITLE_MAX_LENGTH
from kagan.keybindings import REVIEW_BINDINGS
from kagan.limits import AGENT_TIMEOUT
from kagan.ui.utils.animation import WAVE_FRAMES, WAVE_INTERVAL_MS
from kagan.ui.utils.clipboard import copy_with_notification
from kagan.ui.widgets import StreamingOutput

if TYPE_CHECKING:
    from textual.app import ComposeResult
    from textual.timer import Timer

    from kagan.agents.worktree import WorktreeManager
    from kagan.config import AgentConfig
    from kagan.database.models import Ticket

ReviewPhase = Literal["idle", "thinking", "streaming", "complete"]

PHASE_LABELS: dict[ReviewPhase, tuple[str, str]] = {
    "idle": ("○", "Ready"),
    "thinking": (WAVE_FRAMES[0], "Analyzing"),
    "streaming": (WAVE_FRAMES[0], "Streaming"),
    "complete": ("✓", "Complete"),
}


class ReviewModal(ModalScreen[str | None]):
    """Modal for reviewing ticket changes."""

    BINDINGS = REVIEW_BINDINGS

    def __init__(
        self,
        ticket: Ticket,
        worktree_manager: WorktreeManager,
        agent_config: AgentConfig,
        base_branch: str = "main",
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self.ticket = ticket
        self._worktree = worktree_manager
        self._agent_config = agent_config
        self._base_branch = base_branch
        self._agent: Agent | None = None
        self._phase: ReviewPhase = "idle"
        self._diff_stats: str = ""
        self._anim_timer: Timer | None = None
        self._anim_frame: int = 0
        self._prompt_task: asyncio.Task[None] | None = None

    def compose(self) -> ComposeResult:
        with Vertical(id="review-modal-container"):
            yield Label(
                f"Review: {self.ticket.title[:MODAL_TITLE_MAX_LENGTH]}", classes="modal-title"
            )
            yield Rule()

            with Vertical(id="commits-section"):
                yield Label("Commits", classes="section-title")
                yield RichLog(id="commits-log", wrap=True, markup=True)

            with Vertical(id="diff-section"):
                yield Label("Changes", classes="section-title")
                yield Static(id="diff-stats")

            yield Rule()

            with Vertical(id="ai-review-section"):
                with Horizontal(id="ai-review-header"):
                    yield Label("AI Review", classes="section-title")
                    yield Static("", classes="spacer")
                    yield Static("○ Ready", id="phase-badge", classes="phase-badge phase-idle")
                with Horizontal(id="ai-review-controls"):
                    yield Button("Generate Review", id="generate-btn", variant="primary")
                    yield Button("Cancel", id="cancel-btn", variant="warning", classes="hidden")
                    yield Button(
                        "↻ Regenerate", id="regenerate-btn", variant="default", classes="hidden"
                    )
                yield StreamingOutput(id="ai-review-output", classes="hidden")

            yield Rule()

            with Horizontal(classes="button-row"):
                yield Button("Approve", variant="success", id="approve-btn")
                yield Button("Reject", variant="error", id="reject-btn")

        yield Footer()

    async def on_mount(self) -> None:
        """Load commits and diff immediately."""
        commits = await self._worktree.get_commit_log(self.ticket.id, self._base_branch)
        diff_stats = await self._worktree.get_diff_stats(self.ticket.id, self._base_branch)

        log = self.query_one("#commits-log", RichLog)
        for commit in commits or ["[dim]No commits found[/dim]"]:
            log.write(f"  {commit}")

        self.query_one("#diff-stats", Static).update(diff_stats or "[dim](No changes)[/dim]")
        self._diff_stats = diff_stats or ""

    def _set_phase(self, phase: ReviewPhase) -> None:
        """Update phase and UI state."""
        self._phase = phase
        icon, label = PHASE_LABELS[phase]
        badge = self.query_one("#phase-badge", Static)
        badge.update(f"{icon} {label}")
        badge.set_classes(f"phase-badge phase-{phase}")

        gen_btn = self.query_one("#generate-btn", Button)
        cancel_btn = self.query_one("#cancel-btn", Button)
        regen_btn = self.query_one("#regenerate-btn", Button)

        if phase == "idle":
            self._stop_animation()
            gen_btn.remove_class("hidden")
            gen_btn.disabled = False
            cancel_btn.add_class("hidden")
            regen_btn.add_class("hidden")
        elif phase in ("thinking", "streaming"):
            self._start_animation()
            gen_btn.add_class("hidden")
            cancel_btn.remove_class("hidden")
            regen_btn.add_class("hidden")
        else:  # complete
            self._stop_animation()
            gen_btn.add_class("hidden")
            cancel_btn.add_class("hidden")
            regen_btn.remove_class("hidden")

    def _start_animation(self) -> None:
        """Start wave animation for thinking/streaming state."""
        if self._anim_timer is None:
            self._anim_frame = 0
            self._anim_timer = self.set_interval(WAVE_INTERVAL_MS / 1000, self._next_frame)

    def _stop_animation(self) -> None:
        """Stop wave animation."""
        if self._anim_timer is not None:
            self._anim_timer.stop()
            self._anim_timer = None

    def _next_frame(self) -> None:
        """Advance to next animation frame."""
        self._anim_frame = (self._anim_frame + 1) % len(WAVE_FRAMES)
        _, label = PHASE_LABELS[self._phase]
        badge = self.query_one("#phase-badge", Static)
        badge.update(f"{WAVE_FRAMES[self._anim_frame]} {label}")

    @on(Button.Pressed, "#generate-btn")
    async def on_generate_btn(self) -> None:
        await self.action_generate_review()

    @on(Button.Pressed, "#regenerate-btn")
    async def on_regenerate_btn(self) -> None:
        await self.action_regenerate_review()

    @on(Button.Pressed, "#cancel-btn")
    async def on_cancel_btn(self) -> None:
        await self.action_cancel_review()

    @on(Button.Pressed, "#approve-btn")
    def on_approve_btn(self) -> None:
        self.action_approve()

    @on(Button.Pressed, "#reject-btn")
    def on_reject_btn(self) -> None:
        self.action_reject()

    async def action_generate_review(self) -> None:
        """Generate or regenerate AI review."""
        if self._phase == "complete":
            await self.action_regenerate_review()
            return
        if self._phase != "idle":
            return

        self._set_phase("thinking")
        output = self.query_one("#ai-review-output", StreamingOutput)
        output.remove_class("hidden")
        await self._generate_ai_review(output)

    async def action_regenerate_review(self) -> None:
        """Regenerate AI review."""
        if self._phase != "complete":
            return

        if self._agent:
            await self._agent.stop()
            self._agent = None

        output = self.query_one("#ai-review-output", StreamingOutput)
        await output.clear()
        self._set_phase("thinking")
        await self._generate_ai_review(output)

    async def action_cancel_review(self) -> None:
        """Cancel ongoing review."""
        if self._phase not in ("thinking", "streaming"):
            return

        if self._prompt_task and not self._prompt_task.done():
            self._prompt_task.cancel()
        if self._agent:
            await self._agent.stop()
            self._agent = None

        output = self.query_one("#ai-review-output", StreamingOutput)
        await output.post_note("Review cancelled", classes="dismissed")
        self._set_phase("idle")

    async def _generate_ai_review(self, output: StreamingOutput) -> None:
        """Spawn agent to generate code review."""
        wt_path = await self._worktree.get_path(self.ticket.id)
        if not wt_path:
            await output.post_note("Error: Worktree not found", classes="error")
            self._set_phase("idle")
            return

        diff = await self._worktree.get_diff(self.ticket.id, self._base_branch)
        if not diff:
            await output.post_note("No diff to review", classes="info")
            self._set_phase("idle")
            return

        self._agent = Agent(wt_path, self._agent_config, read_only=True)
        self._agent.start(self)

        await output.post_note("Analyzing changes...", classes="info")

        try:
            await self._agent.wait_ready(timeout=AGENT_TIMEOUT)
        except Exception as e:
            await output.post_note(f"Review failed: {e}", classes="error")
            self._set_phase("idle")
            return

        review_prompt = f"""You are a Code Review Specialist providing feedback on changes.

## Context

**Ticket:** {self.ticket.title}

## Changes to Review

```diff
{diff[:DIFF_MAX_LENGTH]}
```

## Review Focus

Evaluate the changes for:
1. Code quality and adherence to best practices
2. Potential bugs or logic errors
3. Test coverage considerations
4. Actionable suggestions for improvement

## Workflow

First, identify what the changes accomplish.
Then, note any issues or improvements.
Finally, provide a concise summary with specific recommendations.

Keep your review brief and actionable."""

        # Fire-and-forget: don't await so UI can update during streaming
        self._prompt_task = asyncio.create_task(self._run_prompt(review_prompt, output))

    async def _run_prompt(self, prompt: str, output: StreamingOutput) -> None:
        """Run prompt in background, handle errors."""
        try:
            await self._agent.send_prompt(prompt)
        except Exception as e:
            await output.post_note(f"Review failed: {e}", classes="error")
            self._set_phase("idle")

    # ACP Message handlers

    @on(messages.AgentUpdate)
    async def on_agent_update(self, message: messages.AgentUpdate) -> None:
        """Handle agent text output."""
        if self._phase == "thinking":
            self._set_phase("streaming")
        output = self.query_one("#ai-review-output", StreamingOutput)
        await output.post_response(message.text)

    @on(messages.Thinking)
    async def on_agent_thinking(self, message: messages.Thinking) -> None:
        """Handle agent thinking."""
        output = self.query_one("#ai-review-output", StreamingOutput)
        await output.post_thought(message.text)

    @on(messages.AgentComplete)
    async def on_agent_complete(self, _: messages.AgentComplete) -> None:
        """Handle agent completion."""
        self._set_phase("complete")

    @on(messages.AgentFail)
    async def on_agent_fail(self, message: messages.AgentFail) -> None:
        """Handle agent failure."""
        output = self.query_one("#ai-review-output", StreamingOutput)
        await output.post_note(f"Error: {message.message}", classes="error")
        self._set_phase("idle")

    def action_approve(self) -> None:
        """Approve the review."""
        self.dismiss("approve")

    def action_reject(self) -> None:
        """Reject the review."""
        self.dismiss("reject")

    async def action_close_or_cancel(self) -> None:
        """Cancel review if in progress, otherwise close."""
        if self._phase in ("thinking", "streaming"):
            await self.action_cancel_review()
        else:
            self.dismiss(None)

    def action_copy(self) -> None:
        """Copy review content to clipboard."""
        output = self.query_one("#ai-review-output", StreamingOutput)
        review_text = output._agent_response._markdown if output._agent_response else ""

        content_parts = [f"# Review: {self.ticket.title}"]
        if self._diff_stats:
            content_parts.append(f"\n## Changes\n{self._diff_stats}")
        if review_text:
            content_parts.append(f"\n## AI Review\n{review_text}")

        copy_with_notification(self.app, "\n".join(content_parts), "Review")

    async def on_unmount(self) -> None:
        """Cleanup agent and animation on close."""
        self._stop_animation()
        if self._prompt_task and not self._prompt_task.done():
            self._prompt_task.cancel()
        if self._agent:
            await self._agent.stop()
