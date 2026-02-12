"""TaskCard widget for displaying a Kanban task."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from textual.containers import Horizontal, Vertical
from textual.message import Message
from textual.reactive import reactive, var
from textual.widget import Widget
from textual.widgets import Label

from kagan.core.constants import (
    CARD_BACKEND_MAX_LENGTH,
    CARD_DESC_MAX_LENGTH,
    CARD_ID_MAX_LENGTH,
    CARD_TITLE_LINE_WIDTH,
)
from kagan.core.models.enums import CardIndicator
from kagan.tui.ui.card_formatters import truncate_text

if TYPE_CHECKING:
    from textual import events
    from textual.app import ComposeResult

    from kagan.core.adapters.db.schema import Task


class TaskCard(Widget):
    """A card widget representing a single task on the Kanban board."""

    can_focus = True

    task_model: reactive[Task | None] = reactive(None)
    is_agent_active: var[bool] = var(False, toggle_class="agent-active", always_update=True)
    indicator: var[CardIndicator] = var(CardIndicator.NONE, always_update=True)

    @dataclass
    class Selected(Message):
        task: Task

    def __init__(self, task: Task, **kwargs) -> None:
        super().__init__(id=f"card-{task.id}", **kwargs)
        self._indicator_label: Label | None = None
        self._title_label: Label | None = None
        self._task_id_label: Label | None = None
        self._description_label: Label | None = None
        self._backend_label: Label | None = None
        self._branch_label: Label | None = None
        self._type_label: Label | None = None
        self._priority_label: Label | None = None
        self.task_model = task

    def compose(self) -> ComposeResult:
        """Compose static card layout and update content incrementally via watchers."""
        with Vertical():
            with Horizontal(classes="card-row"):
                self._indicator_label = Label("", classes="card-indicator")
                self._indicator_label.display = False
                self._title_label = Label("", classes="card-title")
                self._task_id_label = Label("", classes="card-id")
                yield self._indicator_label
                yield self._title_label
                yield self._task_id_label

            with Horizontal(classes="card-row"):
                self._description_label = Label("", classes="card-desc")
                yield self._description_label

            with Horizontal(classes="card-row card-badge-row"):
                self._backend_label = Label("", classes="card-badge card-badge-backend")
                self._backend_label.display = False
                self._branch_label = Label("", classes="card-badge card-branch")
                self._branch_label.display = False
                self._type_label = Label("", classes="card-badge card-badge-type")
                self._priority_label = Label("", classes="card-badge card-badge-priority")
                yield self._backend_label
                yield self._branch_label
                yield self._type_label
                yield Label("", classes="card-spacer")
                yield self._priority_label

    def on_mount(self) -> None:
        self._render_task_model()
        self._render_indicator()

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
        self._render_task_model()

    def watch_is_agent_active(self, active: bool) -> None:
        del active

    def watch_indicator(self, value: CardIndicator) -> None:
        del value
        self._render_indicator()

    def _render_task_model(self) -> None:
        if self.task_model is None:
            return
        if not self._labels_ready():
            return

        assert self._title_label is not None
        assert self._task_id_label is not None
        assert self._description_label is not None
        assert self._backend_label is not None
        assert self._branch_label is not None
        assert self._type_label is not None
        assert self._priority_label is not None

        task = self.task_model
        self._task_id_label.update(f"#{task.short_id[:CARD_ID_MAX_LENGTH]}")
        self._title_label.update(truncate_text(task.title, CARD_TITLE_LINE_WIDTH))

        desc = (task.description or "").strip()
        if desc:
            self._description_label.update(truncate_text(desc, CARD_DESC_MAX_LENGTH))
            self._description_label.remove_class("card-desc-empty")
        else:
            self._description_label.update("No description...")
            self._description_label.add_class("card-desc-empty")

        backend_label = self._get_backend_label()
        if backend_label:
            backend_text = truncate_text(backend_label, CARD_BACKEND_MAX_LENGTH)
            self._backend_label.update(backend_text)
            self._backend_label.display = True
        else:
            self._backend_label.update("")
            self._backend_label.display = False

        if task.base_branch:
            self._branch_label.update(f"⎇ {task.base_branch}")
            self._branch_label.display = True
        else:
            self._branch_label.update("")
            self._branch_label.display = False

        self._type_label.update(task.task_type.value)

        self._priority_label.update(task.priority.label)
        self._priority_label.remove_class("priority-low")
        self._priority_label.remove_class("priority-medium")
        self._priority_label.remove_class("priority-high")
        self._priority_label.add_class(f"priority-{task.priority.css_class}")

    def _render_indicator(self) -> None:
        if self._indicator_label is None:
            return

        label = self._indicator_label
        label.remove_class("indicator-running")
        label.remove_class("indicator-idle")
        label.remove_class("indicator-reviewing")
        label.remove_class("indicator-blocked")
        label.remove_class("indicator-passed")
        label.remove_class("indicator-failed")

        if self.indicator is CardIndicator.NONE:
            label.update("")
            label.display = False
            return

        label.update(f"{self.indicator.icon} ")
        label.add_class(self.indicator.css_class)
        label.display = True

    def _labels_ready(self) -> bool:
        return all(
            (
                self._title_label is not None,
                self._task_id_label is not None,
                self._description_label is not None,
                self._backend_label is not None,
                self._branch_label is not None,
                self._type_label is not None,
                self._priority_label is not None,
            )
        )
