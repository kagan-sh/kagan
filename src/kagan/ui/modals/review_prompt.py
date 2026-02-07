"""Prompt and parsing helpers for the review modal."""

from __future__ import annotations

import asyncio
import re
from typing import TYPE_CHECKING

from kagan.constants import DIFF_MAX_LENGTH
from kagan.core.models.enums import StreamPhase
from kagan.limits import AGENT_TIMEOUT

if TYPE_CHECKING:
    from kagan.acp.agent import Agent
    from kagan.ui.modals.review import ReviewModal
    from kagan.ui.widgets import StreamingOutput

DECISION_PATTERN = re.compile(
    r"^\s*Decision\s*:\s*(?P<decision>approve|approved|reject|rejected)\b",
    re.IGNORECASE | re.MULTILINE,
)
APPROVE_SIGNAL_PATTERN = re.compile(r"<\s*approve\b", re.IGNORECASE)
REJECT_SIGNAL_PATTERN = re.compile(r"<\s*reject\b", re.IGNORECASE)


def extract_review_decision(text: str) -> str | None:
    """Extract terminal review decision from streamed content."""
    if not text:
        return None

    # Parse recent output only to avoid stale decisions in long histories.
    tail = text[-8000:]
    events: list[tuple[int, str]] = []
    for match in DECISION_PATTERN.finditer(tail):
        token = match.group("decision").lower()
        decision = "approved" if token.startswith("approve") else "rejected"
        events.append((match.start(), decision))
    for match in APPROVE_SIGNAL_PATTERN.finditer(tail):
        events.append((match.start(), "approved"))
    for match in REJECT_SIGNAL_PATTERN.finditer(tail):
        events.append((match.start(), "rejected"))

    if not events:
        return None

    return max(events, key=lambda item: item[0])[1]


def truncate_queue_payload(content: str, max_chars: int = 8000) -> str:
    """Keep newest queued context when follow-ups exceed prompt budget."""
    if len(content) <= max_chars:
        return content
    head = "[queued context truncated]\n"
    tail = content[-(max_chars - len(head)) :]
    return f"{head}{tail}"


def build_review_prompt(task_title: str, diff: str, queued_follow_up: str | None) -> str:
    follow_up_context = ""
    if queued_follow_up:
        follow_up_context = (
            "\n\n## Queued User Follow-up\n"
            "Apply this additional context while reviewing:\n"
            f"{queued_follow_up}\n"
        )

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

**Task:** {task_title}

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
    return f"{review_prompt}{follow_up_context}"


class ReviewPromptMixin:
    """Prompt lifecycle for review generation."""

    _agent: Agent | None
    _prompt_task: asyncio.Task[None] | None

    async def _generate_ai_review(self: ReviewModal, output: StreamingOutput) -> None:
        """Spawn agent to generate code review."""
        from kagan.debug_log import log

        wt_path = await self._worktree.get_path(self._task_model.id)
        if not wt_path:
            await output.post_note("Error: Worktree not found", classes="error")
            self._set_phase(StreamPhase.IDLE)
            return

        diff = self._diff_text or await self._worktree.get_diff(
            self._task_model.id, self._base_branch
        )
        if not diff:
            await output.post_note("No diff to review", classes="info")
            self._set_phase(StreamPhase.IDLE)
            return

        queued_follow_up = await self._take_review_queue()

        self._agent = self._agent_factory(wt_path, self._agent_config, read_only=True)
        self._agent.start(self)

        await output.post_note("Analyzing changes...", classes="info")
        log.info("[ReviewModal] Agent started, waiting for response")

        try:
            await self._agent.wait_ready(timeout=AGENT_TIMEOUT)
        except Exception as e:
            await output.post_note(f"Review failed: {e}", classes="error")
            self._set_phase(StreamPhase.IDLE)
            return

        review_prompt = build_review_prompt(self._task_model.title, diff, queued_follow_up)
        self._prompt_task = asyncio.create_task(self._run_prompt(review_prompt, output))

    async def _run_prompt(self: ReviewModal, prompt: str, output: StreamingOutput) -> None:
        """Run prompt in background, handle errors."""
        if self._agent is None:
            return
        try:
            await self._agent.send_prompt(prompt)
        except Exception as e:
            await output.post_note(f"Review failed: {e}", classes="error")
            self._set_phase(StreamPhase.IDLE)
