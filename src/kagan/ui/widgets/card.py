"""TaskCard widget for displaying a Kanban task."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from textual.message import Message
from textual.reactive import reactive, var
from textual.widget import Widget
from textual.widgets import Label

from kagan.constants import (
    BOX_DRAWING,
    CARD_BACKEND_MAX_LENGTH,
    CARD_DESC_MAX_LENGTH,
    CARD_ID_MAX_LENGTH,
    CARD_REVIEW_MAX_LENGTH,
    CARD_TITLE_LINE_WIDTH,
)
from kagan.core.models.enums import TaskPriority, TaskStatus, TaskType
from kagan.ui.card_formatters import (
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


# Progress bar width for agent iteration display
PROGRESS_BAR_WIDTH = 16


def _format_priority_badge(priority: TaskPriority) -> str:
    """Format priority as a badge like [HIGH], [MED], [LOW]."""
    labels = {
        TaskPriority.HIGH: "[HIGH]",
        TaskPriority.MEDIUM: "[MED]",
        TaskPriority.LOW: "[LOW]",
    }
    return labels.get(priority, "[MED]")


def _format_progress_bar(current: int, total: int, width: int = PROGRESS_BAR_WIDTH) -> str:
    """Format progress bar: ━━━━━━━━░░░░░░ 3/5."""
    filled = int((current / total) * width) if total > 0 else 0
    empty = width - filled
    return f"{'━' * filled}{'░' * empty} {current}/{total}"


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
        """Compose the card layout with improved visual hierarchy.

        Layout structure:
        ┌─────────────────────────────────────┐
        │ ⚡ IMPLEMENT USER AUTH        [MED] │  ← Type badge + Title + Priority badge
        │ ────────────────────────────────── │  ← Subtle divider
        │ Add JWT token validation to API    │  ← Description (truncated)
        │                                    │
        │ ▸ claude                    #abc1  │  ← Agent/backend + Short ID
        │ ━━━━━━━━━░░░░░░░ 3/5              │  ← Progress bar (if running)
        └─────────────────────────────────────┘
        """
        if self.task_model is None:
            return

        # === Row 1: Type badge + Title + Priority badge ===
        type_badge = self._get_type_badge()
        priority_badge = _format_priority_badge(self.task_model.priority)

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
            first_line = f"{type_badge} {title_text} {priority_badge}"
        yield Label(first_line, classes="card-title")

        # Second line for long titles (indented to align with first line)
        if len(title_lines) > 1:
            yield Label(f"  {title_lines[1]}", classes="card-title-continued")

        # === Row 2: Subtle divider ===
        divider_char = BOX_DRAWING["THIN_H"]
        yield Label(divider_char * 30, classes="card-divider")

        # === Row 3: Description (truncated with ellipsis) ===
        desc = self.task_model.description or "No description"
        desc_text = truncate_text(desc, CARD_DESC_MAX_LENGTH + 10)  # Slightly wider for desc
        priority_class = self._get_priority_class()
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

        # === Row 4: Agent/backend indicator + Short ID ===
        session_indicator = self._get_session_indicator()
        task_id = f"#{self.task_model.short_id[:CARD_ID_MAX_LENGTH]}"
        backend = getattr(self.task_model, "agent_backend", None) or ""

        # Format: ▸ claude                    #abc1
        agent_display = ""
        if session_indicator:
            agent_display = f"{session_indicator} "
        if backend:
            agent_display += f"▸ {backend[:CARD_BACKEND_MAX_LENGTH]}"
        elif self.task_model.assigned_hat:
            agent_display += f"▸ {self.task_model.assigned_hat[:CARD_BACKEND_MAX_LENGTH]}"

        # Right-align the task ID
        if agent_display:
            meta_text = f"{agent_display.ljust(20)}{task_id}"
        else:
            meta_text = f"{''.ljust(20)}{task_id}"
        yield Label(meta_text, classes="card-meta")

        # === Row 5: Progress bar (only when agent is running) ===
        if self.iteration_info:
            progress_text = self._format_iteration_progress(self.iteration_info)
            yield Label(progress_text, classes="card-iteration")

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

    def _format_iteration_progress(self, iteration_info: str) -> str:
        """Format iteration info as progress bar: ━━━━━━━━░░░░░░ 3/5."""
        if not iteration_info:
            return ""

        # Parse "Iter X/Y" format
        try:
            parts = iteration_info.split()
            if len(parts) >= 2 and "/" in parts[1]:
                current_str, total_str = parts[1].split("/")
                current = int(current_str)
                total = int(total_str)
                return _format_progress_bar(current, total)
        except (ValueError, IndexError):
            pass

        # Fallback: return original info if parsing fails
        return iteration_info

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
