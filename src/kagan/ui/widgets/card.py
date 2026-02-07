"""TaskCard widget for displaying a Kanban task."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from textual.containers import Horizontal, Vertical
from textual.message import Message
from textual.reactive import reactive, var
from textual.widget import Widget
from textual.widgets import Label

from kagan.constants import (
    CARD_BACKEND_MAX_LENGTH,
    CARD_DESC_MAX_LENGTH,
    CARD_ID_MAX_LENGTH,
    CARD_TITLE_LINE_WIDTH,
)
from kagan.core.models.enums import CardIndicator
from kagan.ui.card_formatters import truncate_text

if TYPE_CHECKING:
    from textual import events
    from textual.app import ComposeResult

    from kagan.core.models.entities import Task


class TaskCard(Widget):
    """A card widget representing a single task on the Kanban board."""

    can_focus = True

    task_model: reactive[Task | None] = reactive(None, recompose=True)
    is_agent_active: var[bool] = var(False, toggle_class="agent-active", always_update=True)
    indicator: var[CardIndicator] = var(CardIndicator.NONE, always_update=True)

    @dataclass
    class Selected(Message):
        task: Task

    def __init__(self, task: Task, **kwargs) -> None:
        super().__init__(id=f"card-{task.id}", **kwargs)
        self.task_model = task

    def compose(self) -> ComposeResult:
        """Compose the card layout (Draft E)."""
        if self.task_model is None:
            return

        task_id = f"#{self.task_model.short_id[:CARD_ID_MAX_LENGTH]}"
        title_text = truncate_text(self.task_model.title, CARD_TITLE_LINE_WIDTH)
        type_badge = self.task_model.task_type.value
        priority_badge = self.task_model.priority.label

        desc = (self.task_model.description or "").strip()
        desc_empty = not desc
        desc_text = truncate_text(desc, CARD_DESC_MAX_LENGTH) if desc else "No description..."

        backend_label = self._get_backend_label()
        backend_text = (
            truncate_text(backend_label, CARD_BACKEND_MAX_LENGTH) if backend_label else ""
        )

        with Vertical():
            with Horizontal(classes="card-row"):
                if self.indicator != CardIndicator.NONE:
                    yield Label(
                        f"{self.indicator.icon} ",
                        classes=f"card-indicator {self.indicator.css_class}",
                    )
                yield Label(title_text, classes="card-title")
                yield Label(task_id, classes="card-id")

            with Horizontal(classes="card-row"):
                desc_classes = "card-desc card-desc-empty" if desc_empty else "card-desc"
                yield Label(desc_text, classes=desc_classes)

            with Horizontal(classes="card-row card-badge-row"):
                if backend_text:
                    yield Label(backend_text, classes="card-badge card-badge-backend")
                if self.task_model.base_branch:
                    yield Label(
                        f"⎇ {self.task_model.base_branch}", classes="card-badge card-branch"
                    )
                yield Label(type_badge, classes="card-badge card-badge-type")
                yield Label("", classes="card-spacer")
                yield Label(
                    priority_badge,
                    classes=(
                        "card-badge card-badge-priority "
                        f"priority-{self.task_model.priority.css_class}"
                    ),
                )

    def _get_backend_label(self) -> str:
        if self.task_model is None:
            return ""
        config = getattr(self.app, "config", None)
        if config is None:
            return self.task_model.agent_backend or ""

        agent_config = self.task_model.get_agent_config(config)
        label = agent_config.name if agent_config else ""
        return label.removesuffix(" Code")

    def on_click(self, event: events.Click) -> None:
        """Handle click: single-click focuses, double-click opens details."""
        if event.chain == 1:
            self.focus()
        elif event.chain >= 2 and self.task_model:
            self.post_message(self.Selected(self.task_model))

    def watch_task_model(self, task: Task | None) -> None:
        self.refresh(recompose=True)

    def watch_is_agent_active(self, active: bool) -> None:
        self.refresh(recompose=True)

    def watch_indicator(self, value: CardIndicator) -> None:
        self.refresh(recompose=True)
