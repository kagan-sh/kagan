"""KanbanColumn widget for displaying a status column."""

from __future__ import annotations

from itertools import pairwise
from typing import TYPE_CHECKING

from textual.containers import Container, ScrollableContainer, Vertical
from textual.css.query import NoMatches
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Label

from kagan.core.constants import STATUS_LABELS
from kagan.core.models.enums import CardIndicator, TaskStatus
from kagan.tui.ui.widgets.card import TaskCard

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.core.adapters.db.schema import Task


# Column icons for each status
STATUS_ICONS: dict[TaskStatus, str] = {
    TaskStatus.BACKLOG: "☐",
    TaskStatus.IN_PROGRESS: "▶",
    TaskStatus.REVIEW: "◎",
    TaskStatus.DONE: "✓",
}


class _NSLabel(Label):
    ALLOW_SELECT = False
    can_focus = False


class _NSVertical(Vertical):
    ALLOW_SELECT = False
    can_focus = False


class _NSScrollable(ScrollableContainer):
    ALLOW_SELECT = False
    can_focus = False


class _NSContainer(Container):
    ALLOW_SELECT = False
    can_focus = False


class KanbanColumn(Widget):
    ALLOW_SELECT = False
    can_focus = False

    status: reactive[TaskStatus] = reactive(TaskStatus.BACKLOG)

    def __init__(self, status: TaskStatus, tasks: list[Task] | None = None, **kwargs) -> None:
        super().__init__(id=f"column-{status.value.lower()}", **kwargs)
        self.status = status
        self._tasks: list[Task] = tasks or []
        self._blocked_count: int = 0
        self._has_active_agents: bool = False

    def compose(self) -> ComposeResult:
        with _NSVertical():
            with _NSVertical(classes="column-header"):
                yield _NSLabel(
                    self._header_text(),
                    id=f"header-{self.status.value.lower()}",
                    classes="column-header-text",
                )
            with _NSScrollable(classes="column-content", id=f"content-{self.status.value.lower()}"):
                if self._tasks:
                    for task in self._tasks:
                        yield TaskCard(task)
                else:
                    empty_id = f"empty-{self.status.value.lower()}"
                    with _NSContainer(classes="column-empty", id=empty_id):
                        yield _NSLabel("No tasks", classes="empty-message")

    def get_cards(self) -> list[TaskCard]:
        """Return cards."""
        return list(self.query(TaskCard))

    def get_focused_card_index(self) -> int | None:
        """Return focused card index."""
        for i, card in enumerate(self.get_cards()):
            if card.has_focus:
                return i
        return None

    def focus_card(self, index: int) -> bool:
        cards = self.get_cards()
        if 0 <= index < len(cards):
            cards[index].focus()
            return True
        return False

    def focus_first_card(self) -> bool:
        return self.focus_card(0)

    def update_tasks(self, tasks: list[Task]) -> None:
        """Update tasks with minimal DOM changes - no full recompose."""
        new_tasks = [t for t in tasks if t.status == self.status]
        self._tasks = new_tasks

        try:
            header = self.query_one(f"#header-{self.status.value.lower()}", _NSLabel)
            header.update(self._header_text())
        except NoMatches:
            pass

        current_cards = {card.task_model.id: card for card in self.get_cards() if card.task_model}
        new_tasks_by_id = {t.id: t for t in new_tasks}
        new_task_ids = set(new_tasks_by_id.keys())
        current_ids = set(current_cards.keys())

        try:
            content = self.query_one(f"#content-{self.status.value.lower()}", _NSScrollable)
        except NoMatches:
            return
        if not content.is_attached:
            return

        for task_id in current_ids - new_task_ids:
            card = current_cards[task_id]
            card.remove()

        for task_id in current_ids & new_task_ids:
            card = current_cards[task_id]
            new_task = new_tasks_by_id[task_id]

            card.task_model = new_task

        for task in new_tasks:
            if task.id not in current_ids:
                card = TaskCard(task)
                current_cards[task.id] = card
                content.mount(card)

        # Preserve existing card widgets but ensure visual ordering matches task order.
        ordered_cards = [current_cards[task.id] for task in new_tasks if task.id in current_cards]
        attached_cards = [card for card in ordered_cards if card in content.children]
        if attached_cards:
            first_task_card = next(
                (child for child in content.children if isinstance(child, TaskCard)),
                None,
            )
            if first_task_card is not None and first_task_card is not attached_cards[0]:
                content.move_child(attached_cards[0], before=first_task_card)
            for previous, card in pairwise(attached_cards):
                content.move_child(card, after=previous)

        empty_id = f"empty-{self.status.value.lower()}"
        has_empty = False
        try:
            empty_container = self.query_one(f"#{empty_id}", _NSContainer)
            has_empty = True
            if new_tasks:
                empty_container.remove()
        except NoMatches:
            pass

        if not new_tasks and not has_empty:
            empty = _NSContainer(
                _NSLabel("No tasks", classes="empty-message"),
                classes="column-empty",
                id=empty_id,
            )
            content.mount(empty)

    def update_blocked_count(self, blocked_count: int) -> None:
        if self.status is not TaskStatus.BACKLOG:
            return
        self._blocked_count = max(0, blocked_count)
        try:
            header = self.query_one(f"#header-{self.status.value.lower()}", _NSLabel)
            header.update(self._header_text())
        except NoMatches:
            pass

    def update_active_states(
        self,
        active_ids: set[str],
        indicator_map: dict[str, CardIndicator] | None = None,
    ) -> None:
        """Update active agent state and card indicators for all cards."""
        had_active = self._has_active_agents
        self._has_active_agents = any(
            card.task_model is not None and card.task_model.id in active_ids
            for card in self.query(TaskCard)
        )

        if had_active != self._has_active_agents:
            try:
                header = self.query_one(f"#header-{self.status.value.lower()}", _NSLabel)
                header.update(self._header_text())
            except NoMatches:
                pass

        for card in self.query(TaskCard):
            if card.task_model is not None:
                card.is_agent_active = card.task_model.id in active_ids
                if indicator_map is not None:
                    card.indicator = indicator_map.get(card.task_model.id, CardIndicator.NONE)

    def _header_text(self) -> str:
        icon = STATUS_ICONS.get(self.status, "○")
        text = f"{icon} {STATUS_LABELS[self.status]} ({len(self._tasks)})"
        if self.status is TaskStatus.BACKLOG and self._blocked_count > 0:
            return f"{text} • blocked {self._blocked_count}"
        return text
