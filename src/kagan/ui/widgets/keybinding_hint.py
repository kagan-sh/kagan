"""Contextual keybinding hint widget."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.reactive import reactive
from textual.widgets import Static

if TYPE_CHECKING:
    from textual.app import ComposeResult


class KeybindingHint(Static):
    """Shows contextual keybinding hints based on current focus."""

    hints: reactive[str] = reactive("")

    def compose(self) -> ComposeResult:
        yield from ()

    def watch_hints(self, hints: str) -> None:
        """Update displayed hints."""
        self.update(hints)

    def show_hints(self, hint_list: list[tuple[str, str]]) -> None:
        """Show list of (key, description) hints.

        Example: [("n", "new task"), ("e", "edit"), ("Enter", "start")]
        """
        if not hint_list:
            self.hints = ""
            return

        formatted = " | ".join(f"[bold]{key}[/] {desc}" for key, desc in hint_list)
        self.hints = f"💡 {formatted}"
