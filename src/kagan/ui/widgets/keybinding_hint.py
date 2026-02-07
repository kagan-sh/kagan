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
        self._render_hints(hint_list)

    def _render_hints(self, hint_list: list[tuple[str, str]]) -> None:
        """Render hint list to formatted string.

        Args:
            hint_list: List of (key, description) tuples.
        """
        parts = []
        for key, desc in hint_list:
            if not key:
                continue
            if desc:
                parts.append(f"[bold]{key}[/] {desc}")
            else:
                # Key-only hint (like "?" for help)
                parts.append(f"[bold]{key}[/]")

        formatted = " · ".join(parts)
        self.hints = formatted

    def clear(self) -> None:
        """Clear all hints."""
        self.hints = ""
