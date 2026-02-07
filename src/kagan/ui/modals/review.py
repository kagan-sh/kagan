"""Modal for reviewing ticket changes."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Literal

from textual import on
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Footer, Label, LoadingIndicator, RichLog, Rule, Static

from kagan.acp import messages
from kagan.agents.agent_factory import AgentFactory, create_agent
from kagan.constants import DIFF_MAX_LENGTH, MODAL_TITLE_MAX_LENGTH
from kagan.keybindings import REVIEW_BINDINGS
from kagan.limits import AGENT_TIMEOUT
from kagan.ui.utils.animation import WAVE_FRAMES, WAVE_INTERVAL_MS
from kagan.ui.utils.clipboard import copy_with_notification
from kagan.ui.widgets import StreamingOutput

if TYPE_CHECKING:
    from textual.app import ComposeResult
    from textual.timer import Timer

    from kagan.acp.agent import Agent
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
        agent_factory: AgentFactory = create_agent,
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self.ticket = ticket
        self._worktree = worktree_manager
        self._agent_config = agent_config
        self._base_branch = base_branch
        self._agent_factory = agent_factory
        self._agent: Agent | None = None
        self._phase: ReviewPhase = "idle"
        self._diff_stats: str = ""
        self._no_changes: bool = False
        self._approve_after_complete: bool = False
        self._anim_timer: Timer | None = None
        self._anim_frame: int = 0
        self._prompt_task: asyncio.Task[None] | None = None

    def compose(self) -> ComposeResult:
        with Vertical(id="review-modal-container"):
            yield Label(
                f"Review: {self.ticket.title[:MODAL_TITLE_MAX_LENGTH]}", classes="modal-title"
            )
            yield Rule()

            yield LoadingIndicator(id="review-loading", classes="review-loading")

            with Vertical(id="commits-section", classes="hidden"):
                yield Label("Commits", classes="section-title")
                yield RichLog(id="commits-log", wrap=True, markup=True)

            with Vertical(id="diff-section", classes="hidden"):
                yield Label("Changes", classes="section-title")
                yield Static(id="diff-stats")

            yield Rule()

            with Vertical(id="ai-review-section", classes="hidden"):
                with Horizontal(id="ai-review-header"):
                    yield Label("AI Review", classes="section-title")
                    yield Static("", classes="spacer")
                    yield Static("○ Ready", id="phase-badge", classes="phase-badge phase-idle")
                with Horizontal(id="ai-review-controls"):
                    yield Button("Generate Review (g)", id="generate-btn", variant="primary")
                    yield Button("Cancel", id="cancel-btn", variant="warning", classes="hidden")
                    yield Button(
                        "↻ Regenerate", id="regenerate-btn", variant="default", classes="hidden"
                    )
                yield StreamingOutput(id="ai-review-output", classes="hidden")

            yield Rule()

            with Horizontal(classes="button-row hidden"):
                yield Button("Approve (a)", variant="success", id="approve-btn")
                yield Button("Reject (r)", variant="error", id="reject-btn")

        yield Footer()

    async def on_mount(self) -> None:
        """Load commits and diff immediately."""
        from kagan.debug_log import log

        log.info(f"[ReviewModal] Opening for ticket {self.ticket.id[:8]}")
        commits = await self._worktree.get_commit_log(self.ticket.id, self._base_branch)

        diff_stats = await self._worktree.get_diff_stats(self.ticket.id, self._base_branch)

        # Hide loading, show content
        self.query_one("#review-loading", LoadingIndicator).display = False
        self.query_one("#commits-section").remove_class("hidden")
        self.query_one("#diff-section").remove_class("hidden")
        self.query_one("#ai-review-section").remove_class("hidden")
        self.query_one(".button-row").remove_class("hidden")

        commits_log = self.query_one("#commits-log", RichLog)
        for commit in commits or ["[dim]No commits found[/dim]"]:
            commits_log.write(f"  {commit}")

        self.query_one("#diff-stats", Static).update(diff_stats or "[dim](No changes)[/dim]")
        self._diff_stats = diff_stats or ""
        self._no_changes = not commits and not self._diff_stats

        log.info(
            f"[ReviewModal] Loaded {len(commits or [])} commits, "
            f"diff_length={len(self._diff_stats)}"
        )

        if self._no_changes:
            approve_btn = self.query_one("#approve-btn", Button)
            approve_btn.label = "Close as Exploratory"

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
        from kagan.debug_log import log

        log.info(f"[ReviewModal] Starting AI review (phase={self._phase})")

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

        self._approve_after_complete = False
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
        from kagan.debug_log import log

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

        self._agent = self._agent_factory(wt_path, self._agent_config, read_only=True)
        self._agent.start(self)

        await output.post_note("Analyzing changes...", classes="info")
        log.info("[ReviewModal] Agent started, waiting for response")

        try:
            await self._agent.wait_ready(timeout=AGENT_TIMEOUT)
        except Exception as e:
            await output.post_note(f"Review failed: {e}", classes="error")
            self._set_phase("idle")
            return

        review_prompt = f"""You are a Code Review Specialist providing feedback on changes.

## Core Principles

- Iterative refinement: inspect, re-check, then summarize.
- Clarity & specificity: concise, unambiguous, actionable.
- Learning by example: follow the example format below.
- Structured reasoning: let's think step by step for complex changes.
- Separate reasoning from the final summary.

## Safety & Secrets

Never access or request secrets/credentials/keys (e.g., `.env`, `.env.*`, `id_rsa`,
`*.pem`, `*.key`, `credentials.json`). If a recommendation requires secrets, ask
for redacted values or suggest safe mocks.

## Context

**Ticket:** {self.ticket.title}

## Changes to Review

```diff
{diff[:DIFF_MAX_LENGTH]}
```

## Output Format

Reasoning:
- 2-5 brief steps that justify your assessment

Findings:
- Specific issues or improvements (if any)

Summary:
- Concise recommendation(s)

## Examples

Example 1: Minor improvement needed
Reasoning:
- Validation was added, but the error message is vague.
Findings:
- Suggest clearer error copy for invalid input.
Summary:
- Solid change; improve error messaging clarity.

Example 2: Potential bug
Reasoning:
- New logic uses `or` where `and` is required for all conditions.
Findings:
- This could bypass required validation in edge cases.
Summary:
- Fix boolean condition before shipping.

Example 3: Missing tests
Reasoning:
- Feature adds a new branch with no coverage.
Findings:
- Add unit tests for the new branch behavior.
Summary:
- Add tests to cover new logic.

Keep your review brief and actionable."""

        # Fire-and-forget: don't await so UI can update during streaming
        self._prompt_task = asyncio.create_task(self._run_prompt(review_prompt, output))

    async def _run_prompt(self, prompt: str, output: StreamingOutput) -> None:
        """Run prompt in background, handle errors."""
        if self._agent is None:
            return
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
        from kagan.debug_log import log

        self._set_phase("complete")

        # If triggered by 'a' key, auto-approve
        if self._approve_after_complete:
            self._approve_after_complete = False
            log.info("[ReviewModal] Auto-approving after AI review completion")
            self.dismiss("approve")

    @on(messages.AgentFail)
    async def on_agent_fail(self, message: messages.AgentFail) -> None:
        """Handle agent failure."""
        output = self.query_one("#ai-review-output", StreamingOutput)
        await output.post_note(f"Error: {message.message}", classes="error")
        self._set_phase("idle")

    def action_approve(self) -> None:
        """Approve review - generates if not done yet."""
        if self._phase == "idle" and not self._no_changes:
            # No review generated yet, start agent first
            self.notify("Starting AI review first...", severity="information")
            self._approve_after_complete = True
            self.run_worker(self.action_generate_review())
        elif self._no_changes:
            self.dismiss("exploratory")
        else:
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
