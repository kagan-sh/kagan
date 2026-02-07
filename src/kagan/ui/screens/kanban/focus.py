"""Focus and navigation utilities for Kanban screen."""

from __future__ import annotations

from typing import TYPE_CHECKING

from kagan.constants import COLUMN_ORDER
from kagan.core.models.enums import TaskStatus
from kagan.ui.widgets.card import TaskCard
from kagan.ui.widgets.column import KanbanColumn

if TYPE_CHECKING:
    from kagan.ui.screens.kanban.screen import KanbanScreen


def get_columns(screen: KanbanScreen) -> list[KanbanColumn]:
    return [screen.query_one(f"#column-{s.value.lower()}", KanbanColumn) for s in COLUMN_ORDER]


def get_focused_card(screen: KanbanScreen) -> TaskCard | None:
    focused = screen.app.focused
    return focused if isinstance(focused, TaskCard) else None


def focus_first_card(screen: KanbanScreen) -> None:
    for col in get_columns(screen):
        if col.focus_first_card():
            return


def focus_column(screen: KanbanScreen, status: TaskStatus) -> None:
    col = screen.query_one(f"#column-{status.value.lower()}", KanbanColumn)
    col.focus_first_card()


def focus_horizontal(screen: KanbanScreen, direction: int) -> None:
    card = get_focused_card(screen)
    columns = get_columns(screen)

    if not card or not card.task_model:
        focus_first_card(screen)
        return

    col_idx = next((i for i, s in enumerate(COLUMN_ORDER) if s == card.task_model.status), -1)
    card_idx = columns[col_idx].get_focused_card_index() or 0

    target_idx = col_idx + direction
    while 0 <= target_idx < len(COLUMN_ORDER):
        cards = columns[target_idx].get_cards()
        if cards:
            columns[target_idx].focus_card(min(card_idx, len(cards) - 1))
            return
        target_idx += direction


def focus_vertical(screen: KanbanScreen, direction: int) -> None:
    card = get_focused_card(screen)

    if not card or not card.task_model:
        focus_first_card(screen)
        return

    status = card.task_model.status
    status_str = status.value if isinstance(status, TaskStatus) else status
    col = screen.query_one(f"#column-{status_str.lower()}", KanbanColumn)
    idx = col.get_focused_card_index()
    cards = col.get_cards()
    if idx is not None:
        new_idx = idx + direction
        if 0 <= new_idx < len(cards):
            col.focus_card(new_idx)
