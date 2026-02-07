"""Widget to display agent plan entries."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Literal, cast

from textual.containers import VerticalGroup
from textual.widgets import Static

from kagan.core.models.enums import PlanStatus
from kagan.ui.utils.clipboard import copy_with_notification

if TYPE_CHECKING:
    from acp.schema import PlanEntry as AcpPlanEntry
    from textual.app import ComposeResult
    from textual.events import Click


class PlanEntry(Static):
    """Single plan entry with double-click to copy."""

    DEFAULT_CLASSES = "plan-entry"

    def __init__(self, entry_content: str, status: PlanStatus) -> None:
        super().__init__()
        self._entry_content = entry_content
        self._status: PlanStatus = status

    @property
    def entry_content(self) -> str:
        return self._entry_content

    @property
    def status(self) -> PlanStatus:
        return self._status

    def update_status(self, status: PlanStatus) -> None:
        self._status = status
        self.refresh()

    def render(self) -> str:
        icon = self._status.icon
        return f"  {icon} {self._entry_content}"

    async def _on_click(self, event: Click) -> None:
        """Handle click events - copy on double-click."""
        if event.chain == 2:
            copy_with_notification(self.app, self._entry_content, "Plan entry")


class PlanDisplay(VerticalGroup):
    """Display agent plan entries with status indicators."""

    def __init__(self, entries: list[AcpPlanEntry], **kwargs) -> None:
        self._entries = entries
        super().__init__(**kwargs)

    @property
    def entries(self) -> list[AcpPlanEntry]:
        return self._entries

    @property
    def has_entries(self) -> bool:
        return len(self._entries) > 0

    def compose(self) -> ComposeResult:
        for entry in self._entries:
            status = PlanStatus(entry.status)
            content = entry.content
            yield PlanEntry(entry_content=content, status=status)

    def update_entries(self, entries: list[AcpPlanEntry]) -> None:
        self._entries = entries
        self.remove_children()
        for entry in self._entries:
            status = PlanStatus(entry.status)
            content = entry.content
            self.mount(PlanEntry(entry_content=content, status=status))

    def update_entry_status(self, index: int, status: PlanStatus) -> None:
        if 0 <= index < len(self._entries):
            entry = self._entries[index]
            # ACP schema doesn't have "failed", map it to "pending"
            entry_status: Literal["pending", "in_progress", "completed"] = (
                "pending" if status == PlanStatus.FAILED else cast("Any", status.value)
            )
            entry.status = entry_status
            children = list(self.query(PlanEntry))
            if 0 <= index < len(children):
                children[index].update_status(status)
