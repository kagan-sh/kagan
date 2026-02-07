"""KanbanColumn widget for displaying a status column."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Container, ScrollableContainer, Vertical
from textual.css.query import NoMatches
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Label

from kagan.constants import STATUS_LABELS
from kagan.core.models.enums import TaskStatus
from kagan.ui.widgets.card import TaskCard

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.core.models.entities import Task


def _get_status_dot(count: int, has_active: bool) -> str:
    """Get status indicator dot based on task count and activity.

    - ● = Column is full (>= 8 items) or has active work (running agents)
    - ◐ = Column is filling up (5-7 items)
    - ○ = Column has capacity (< 5 items)
    """
    if has_active:
        return "●"  # Active agents running
    elif count >= 8:
        return "●"  # Full
    elif count >= 5:
        return "◐"  # Filling up
    else:
        return "○"  # Has capacity


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
        self._has_active_agents: bool = False

    def compose(self) -> ComposeResult:
        status_dot = _get_status_dot(len(self._tasks), self._has_active_agents)
        with _NSVertical():
            with _NSVertical(classes="column-header"):
                yield _NSLabel(
                    f"{status_dot} {STATUS_LABELS[self.status]} ({len(self._tasks)})",
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
        return list(self.query(TaskCard))

    def get_focused_card_index(self) -> int | None:
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
        """Update tasks with minimal DOM changes - no full recompose.

        - Updates existing cards when task metadata changes
        - Adds new cards for tasks that weren't here before
        - Removes cards for tasks that moved out
        """
        new_tasks = [t for t in tasks if t.status == self.status]
        self._tasks = new_tasks

        # Update header count
        try:
            header = self.query_one(f"#header-{self.status.value.lower()}", _NSLabel)
            status_dot = _get_status_dot(len(new_tasks), self._has_active_agents)
            header.update(f"{status_dot} {STATUS_LABELS[self.status]} ({len(new_tasks)})")
        except NoMatches:
            pass

        # Get current cards and build lookup
        current_cards = {card.task_model.id: card for card in self.get_cards() if card.task_model}
        new_tasks_by_id = {t.id: t for t in new_tasks}
        new_task_ids = set(new_tasks_by_id.keys())
        current_ids = set(current_cards.keys())

        try:
            content = self.query_one(f"#content-{self.status.value.lower()}", _NSScrollable)
        except NoMatches:
            return

        # Remove cards for tasks no longer in this column
        for task_id in current_ids - new_task_ids:
            card = current_cards[task_id]
            card.remove()

        # Update existing cards with new task data (handles metadata changes like type)
        for task_id in current_ids & new_task_ids:
            card = current_cards[task_id]
            new_task = new_tasks_by_id[task_id]
            # Update the task reactive - this triggers recompose if needed
            card.task_model = new_task

        # Add new cards only (tasks that weren't here before)
        for task in new_tasks:
            if task.id not in current_ids:
                content.mount(TaskCard(task))

        # Handle empty state container
        empty_id = f"empty-{self.status.value.lower()}"
        has_empty = False
        try:
            empty_container = self.query_one(f"#{empty_id}", _NSContainer)
            has_empty = True
            if new_tasks:
                # Have tasks now, remove empty state
                empty_container.remove()
        except NoMatches:
            pass

        # If no tasks and no empty container, add empty state
        if not new_tasks and not has_empty:
            empty = _NSContainer(classes="column-empty", id=empty_id)
            content.mount(empty)
            empty.mount(_NSLabel("No tasks", classes="empty-message"))

    def update_active_states(self, active_ids: set[str]) -> None:
        """Update active agent state for all cards in this column."""
        # Track whether this column has any active agents
        had_active = self._has_active_agents
        self._has_active_agents = any(
            card.task_model is not None and card.task_model.id in active_ids
            for card in self.query(TaskCard)
        )

        # Update header if active state changed (affects status dot)
        if had_active != self._has_active_agents:
            try:
                header = self.query_one(f"#header-{self.status.value.lower()}", _NSLabel)
                status_dot = _get_status_dot(len(self._tasks), self._has_active_agents)
                header.update(f"{status_dot} {STATUS_LABELS[self.status]} ({len(self._tasks)})")
            except NoMatches:
                pass

        # Update individual card states
        for card in self.query(TaskCard):
            if card.task_model is not None:
                card.is_agent_active = card.task_model.id in active_ids

    def update_iterations(self, iterations: dict[str, str]) -> None:
        """Update iteration display on cards.

        Only updates cards that are in the iterations dict.
        To clear a card's iteration, pass an empty string for that task_id.
        """
        for card in self.query(TaskCard):
            if card.task_model and card.task_model.id in iterations:
                card.iteration_info = iterations[card.task_model.id]

    def update_merge_readiness(self, readiness: dict[str, str]) -> None:
        """Update merge readiness display on cards."""
        for card in self.query(TaskCard):
            if card.task_model and card.task_model.id in readiness:
                card.merge_readiness = readiness[card.task_model.id]
