"""Contextual keybinding hint widget — two-tier variant for Kanban."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Horizontal
from textual.reactive import reactive, var
from textual.widget import Widget
from textual.widgets import Static

if TYPE_CHECKING:
    from textual.app import ComposeResult


def _render_group(hint_list: list[tuple[str, str]], separator: str = " · ") -> str:
    """Render a list of (key, description) pairs to Rich markup."""
    parts = []
    for key, desc in hint_list:
        if not key:
            continue
        if desc:
            parts.append(f"[bold]{key}[/] {desc}")
        else:
            parts.append(f"[bold]{key}[/]")
    return separator.join(parts)


class KeybindingHint(Static):
    """Shows contextual keybinding hints based on current focus.

    Single-line variant used by Welcome, RepoPicker, and other simple screens.
    """

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
        self.hints = _render_group(hint_list)

    def clear(self) -> None:
        """Clear all hints."""
        self.hints = ""


class KanbanHintBar(Widget):
    """Two-row keybinding hint bar for the Kanban board.

    Row 1 (navigation): movement shortcuts — always visible when a card is focused.
    Row 2 (actions): context-sensitive actions per task status/type.

    Replaces both KeybindingHint and Footer on the Kanban screen.
    """

    SCOPED_CSS = False

    has_card: var[bool] = var(False, init=False)

    def compose(self) -> ComposeResult:
        with Horizontal(classes="hint-bar-row hint-bar-nav"):
            yield Static("", id="hint-nav-left", classes="hint-nav-left")
            yield Static("", id="hint-nav-center", classes="hint-nav-center")
            yield Static("", id="hint-nav-right", classes="hint-nav-right")
        with Horizontal(classes="hint-bar-row hint-bar-actions"):
            yield Static("", id="hint-actions-left", classes="hint-actions-left")
            yield Static("", id="hint-actions", classes="hint-actions")
            yield Static("", id="hint-global", classes="hint-global")

    def watch_has_card(self, value: bool) -> None:
        """Toggle CSS class when card selection state changes."""
        self.set_class(value, "card-focused")

    def show_kanban_hints(
        self,
        navigation: list[tuple[str, str]],
        actions: list[tuple[str, str]],
        global_hints: list[tuple[str, str]],
    ) -> None:
        """Update both rows of the hint bar.

        Args:
            navigation: Movement hints for row 1 (left/right arrows).
            actions: Context-sensitive action hints for row 2.
            global_hints: Always-visible global shortcuts (help, actions).
        """
        nav_left = self.query_one("#hint-nav-left", Static)
        nav_center = self.query_one("#hint-nav-center", Static)
        nav_right = self.query_one("#hint-nav-right", Static)
        actions_left = self.query_one("#hint-actions-left", Static)
        actions_widget = self.query_one("#hint-actions", Static)
        global_widget = self.query_one("#hint-global", Static)

        if navigation:
            self.has_card = True
            left_hint = navigation[0] if len(navigation) > 0 else ("", "")
            right_hint = navigation[1] if len(navigation) > 1 else ("", "")
            nav_left.update(
                f"[dim]◀[/] [bold]{left_hint[0]}[/] {left_hint[1]}" if left_hint[0] else ""
            )
            nav_center.update("[dim]h j k l[/dim] navigate")
            nav_right.update(
                f"{right_hint[1]} [bold]{right_hint[0]}[/] [dim]▶[/]" if right_hint[0] else ""
            )
        else:
            self.has_card = False
            nav_left.update("")
            nav_center.update("[dim]h j k l[/dim] navigate")
            nav_right.update("")

        actions_left.update("")
        actions_widget.update(_render_group(actions))
        global_widget.update(_render_group(global_hints, separator="  "))

    def clear(self) -> None:
        """Clear all hints."""
        for widget_id in (
            "#hint-nav-left",
            "#hint-nav-center",
            "#hint-nav-right",
            "#hint-actions-left",
            "#hint-actions",
            "#hint-global",
        ):
            self.query_one(widget_id, Static).update("")
        self.has_card = False
