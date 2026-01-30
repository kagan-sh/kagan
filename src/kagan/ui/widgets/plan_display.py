"""Widget to display agent plan entries."""

from __future__ import annotations

from typing import TYPE_CHECKING, Literal, cast

from textual.containers import VerticalGroup
from textual.widgets import Static

from kagan.ui.utils.clipboard import copy_with_notification

if TYPE_CHECKING:
    from textual.app import ComposeResult
    from textual.events import Click

    from kagan.acp import protocol

STATUS_ICONS: dict[str, str] = {
    "pending": "○",
    "in_progress": "◐",
    "completed": "●",
    "failed": "✗",
}

PlanStatus = Literal["pending", "in_progress", "completed", "failed"]


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
        icon = STATUS_ICONS.get(self._status, "○")
        return f"  {icon} {self._entry_content}"

    async def _on_click(self, event: Click) -> None:
        """Handle click events - copy on double-click."""
        if event.chain == 2:
            copy_with_notification(self.app, self._entry_content, "Plan entry")


class PlanDisplay(VerticalGroup):
    """Display agent plan entries with status indicators."""

    def __init__(self, entries: list[protocol.PlanEntry], **kwargs) -> None:
        self._entries = entries
        super().__init__(**kwargs)

    @property
    def entries(self) -> list[protocol.PlanEntry]:
        return self._entries

    @property
    def has_entries(self) -> bool:
        return len(self._entries) > 0

    def compose(self) -> ComposeResult:
        for entry in self._entries:
            status = cast("PlanStatus", entry.get("status", "pending"))
            content = entry.get("content", "")
            yield PlanEntry(entry_content=content, status=status)

    def update_entries(self, entries: list[protocol.PlanEntry]) -> None:
        self._entries = entries
        self.remove_children()
        for entry in self._entries:
            status = cast("PlanStatus", entry.get("status", "pending"))
            content = entry.get("content", "")
            self.mount(PlanEntry(entry_content=content, status=status))

    def update_entry_status(self, index: int, status: PlanStatus) -> None:
        if 0 <= index < len(self._entries):
            self._entries[index]["status"] = status
            children = list(self.query(PlanEntry))
            if 0 <= index < len(children):
                children[index].update_status(status)
