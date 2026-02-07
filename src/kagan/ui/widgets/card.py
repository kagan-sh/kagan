"""TaskCard widget for displaying a Kanban task."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from textual.message import Message
from textual.reactive import reactive, var
from textual.widget import Widget
from textual.widgets import Label

from kagan.constants import (
    CARD_BACKEND_MAX_LENGTH,
    CARD_DESC_MAX_LENGTH,
    CARD_HAT_MAX_LENGTH,
    CARD_ID_MAX_LENGTH,
    CARD_REVIEW_MAX_LENGTH,
    CARD_TITLE_LINE_WIDTH,
)
from kagan.core.models.enums import TaskStatus, TaskType
from kagan.ui.formatters.card_formatters import (
    format_progress_bar,
    format_review_status,
    get_readiness_badge,
    get_review_badge,
    truncate_text,
    wrap_title,
)

if TYPE_CHECKING:
    from textual import events
    from textual.app import ComposeResult

    from kagan.core.models.entities import Task


class TaskCard(Widget):
    """A card widget representing a single task on the Kanban board."""

    can_focus = True

    task_model: reactive[Task | None] = reactive(None, recompose=True)
    is_agent_active: var[bool] = var(False, toggle_class="agent-active", always_update=True)
    is_session_active: var[bool] = var(False, toggle_class="session-active")
    iteration_info: reactive[str] = reactive("", recompose=True)
    review_state: var[str] = var("")
    merge_readiness: var[str] = var("")

    @dataclass
    class Selected(Message):
        task: Task

    def __init__(self, task: Task, **kwargs) -> None:
        super().__init__(id=f"card-{task.id}", **kwargs)
        self.task_model = task

    def compose(self) -> ComposeResult:
        """Compose the card layout."""
        if self.task_model is None:
            return

        type_badge = self._get_type_badge()
        # Title in uppercase for better hierarchy
        title_lines = wrap_title(self.task_model.title.upper(), CARD_TITLE_LINE_WIDTH)
        title_text = title_lines[0] if title_lines else "UNTITLED"

        if self.task_model.status == TaskStatus.REVIEW:
            review_badge = get_review_badge(self.task_model)
            readiness_badge = get_readiness_badge(
                self.task_model, self.merge_readiness, self.task_model.status
            )
            first_line = f"{type_badge} {title_text} {review_badge} {readiness_badge}"
        else:
            first_line = f"{type_badge} {title_text}"
        yield Label(first_line, classes="card-title")

        # Second line for long titles (indented to align with first line)
        if len(title_lines) > 1:
            yield Label(f"  {title_lines[1]}", classes="card-title-continued")

        # Description line: Priority icon + description
        priority_class = self._get_priority_class()
        priority_icon = {"LOW": "▽", "MED": "◇", "HIGH": "△"}[self.task_model.priority_label]
        desc = self.task_model.description or "No description"
        desc_text = f"{priority_icon} {truncate_text(desc, CARD_DESC_MAX_LENGTH)}"
        yield Label(desc_text, classes=f"card-desc {priority_class}")

        # Error indicator (if last_error exists)
        if self.task_model.last_error:
            error_text = f"🔴 {truncate_text(self.task_model.last_error, CARD_DESC_MAX_LENGTH)}"
            yield Label(error_text, classes="card-error")

        # Block reason (if block_reason exists)
        if self.task_model.block_reason:
            truncated = truncate_text(self.task_model.block_reason, CARD_DESC_MAX_LENGTH)
            block_text = f"🛑 Blocked: {truncated}"
            yield Label(block_text, classes="card-blocked")

        # Iteration info with progress bar (if agent is running)
        if self.iteration_info:
            progress_text = format_progress_bar(self.iteration_info)
            yield Label(progress_text, classes="card-iteration")

        # Review info for REVIEW tasks
        if self.task_model.status == TaskStatus.REVIEW:
            summary = self.task_model.review_summary or "No summary"
            yield Label(
                truncate_text(f"Summary: {summary}", CARD_REVIEW_MAX_LENGTH),
                classes="card-review",
            )
            # Consolidated status line with merge readiness
            status_text = format_review_status(self.task_model, self.merge_readiness)
            readiness_class = f"card-checks readiness-{self.merge_readiness or 'risk'}"
            yield Label(status_text, classes=readiness_class)

        # Metadata footer: grouped in single line with pipe separators
        session_indicator = self._get_session_indicator()
        hat = self.task_model.assigned_hat or ""
        hat_display = hat[:CARD_HAT_MAX_LENGTH] if hat else ""
        task_id = f"#{self.task_model.short_id[:CARD_ID_MAX_LENGTH]}"
        date_str = self.task_model.created_at.strftime("%m.%d")
        backend = getattr(self.task_model, "agent_backend", None) or ""

        meta_parts = []
        if session_indicator:
            meta_parts.append(session_indicator)
        if backend:
            meta_parts.append(backend[:CARD_BACKEND_MAX_LENGTH])
        elif hat_display:
            meta_parts.append(hat_display)
        ac_count = (
            len(self.task_model.acceptance_criteria) if self.task_model.acceptance_criteria else 0
        )
        if ac_count:
            meta_parts.append(f"AC:{ac_count}")
        meta_parts.append(task_id)
        meta_parts.append(date_str)

        meta_text = " | ".join(meta_parts)
        yield Label(meta_text, classes="card-meta")

    def _get_priority_class(self) -> str:
        """Get CSS class for priority."""
        if self.task_model is None:
            return "low"
        return self.task_model.priority.css_class

    def _get_type_badge(self) -> str:
        """Get type badge indicator for task type with agent state."""
        if self.task_model is None:
            return "👤"
        task_type = self.task_model.task_type
        if task_type == TaskType.AUTO:
            # Show running state for AUTO tasks
            if self.is_agent_active:
                return "🔄"  # Running indicator
            if self.task_model.status == TaskStatus.IN_PROGRESS:
                return "⏳"  # Waiting/pending indicator
            return "⚡"  # Normal AUTO badge
        return "👤"  # PAIR mode (human)

    def _get_session_indicator(self) -> str:
        """Get visual indicator for session/agent state."""
        if self.task_model is None:
            return ""

        # Agent actively working - show animated indicator
        if self.is_agent_active:
            return "●"  # Filled circle (will pulse via CSS)

        # tmux session exists but not actively working
        if self.is_session_active:
            return "◉"  # Circle with dot (steady state)

        return ""

    def _update_review_state(self) -> None:
        if self.task_model is None or self.task_model.status != TaskStatus.REVIEW:
            self.review_state = ""
            self.merge_readiness = ""
            return
        if self.task_model.merge_failed:
            self.review_state = "-review-merge-failed"
            self.merge_readiness = "blocked"
        elif self.task_model.checks_passed is True:
            self.review_state = "-review-passed"
        elif self.task_model.checks_passed is False:
            self.review_state = "-review-failed"
        else:
            self.review_state = "-review-pending"
        readiness_value = getattr(self.task_model, "merge_readiness", None)
        if readiness_value:
            self.merge_readiness = (
                readiness_value.value if hasattr(readiness_value, "value") else str(readiness_value)
            )
        if not self.merge_readiness:
            self.merge_readiness = "risk"

    def on_click(self, event: events.Click) -> None:
        """Handle click: single-click focuses, double-click opens details."""
        if event.chain == 1:
            # Single click - just focus
            self.focus()
        elif event.chain >= 2 and self.task_model:
            # Double click - open details
            self.post_message(self.Selected(self.task_model))

    def watch_task_model(self, task: Task | None) -> None:
        if task is None:
            self.is_session_active = False
        else:
            self.is_session_active = task.session_active
        self._update_review_state()

    def watch_review_state(self, old_state: str, new_state: str) -> None:
        if old_state:
            self.remove_class(old_state)
        if new_state:
            self.add_class(new_state)

    def watch_merge_readiness(self, old_state: str, new_state: str) -> None:
        if old_state:
            self.remove_class(f"readiness-{old_state}")
        if new_state:
            self.add_class(f"readiness-{new_state}")

    def watch_is_agent_active(self, active: bool) -> None:
        self.refresh(recompose=True)
