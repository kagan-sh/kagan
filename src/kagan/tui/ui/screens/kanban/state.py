from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Sequence

    from kagan.core.adapters.db.schema import Task
    from kagan.core.models.enums import TaskStatus


@dataclass(slots=True)
class KanbanUiState:
    """Mutable UI state for transient Kanban interactions."""

    filtered_tasks: Sequence[Task] | None = None
    editing_task_id: str | None = None
    pending_delete_task: Task | None = None
    pending_merge_task: Task | None = None
    pending_close_task: Task | None = None
    pending_advance_task: Task | None = None
    pending_auto_move_task: Task | None = None
    pending_auto_move_status: TaskStatus | None = None

    def clear_pending(self) -> None:
        self.pending_delete_task = None
        self.pending_merge_task = None
        self.pending_close_task = None
        self.pending_advance_task = None
        self.pending_auto_move_task = None
        self.pending_auto_move_status = None

    def clear_all(self) -> None:
        self.clear_pending()
        self.editing_task_id = None
        self.filtered_tasks = None
