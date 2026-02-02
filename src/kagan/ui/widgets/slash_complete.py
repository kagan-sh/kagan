"""Slash command autocomplete widget for planner input."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from textual.binding import Binding
from textual.containers import VerticalGroup
from textual.message import Message
from textual.reactive import var
from textual.widgets import OptionList
from textual.widgets.option_list import Option

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.ui.screens.planner.state import SlashCommand


class SlashComplete(VerticalGroup):
    """Autocomplete dropdown for slash commands - just shows options."""

    DEFAULT_CLASSES = "slash-complete"

    BINDINGS = [
        Binding("escape", "dismiss", "Dismiss", show=False),
        Binding("up", "cursor_up", "Up", show=False),
        Binding("down", "cursor_down", "Down", show=False),
        Binding("enter", "select", "Select", show=False),
    ]

    slash_commands: var[list[SlashCommand]] = var(list, init=False)

    @dataclass
    class Completed(Message):
        """Posted when user selects a command."""

        command: str

    @dataclass
    class Dismissed(Message):
        """Posted when user presses Escape."""

        pass

    def __init__(
        self,
        *,
        id: str | None = None,
        classes: str | None = None,
    ) -> None:
        super().__init__(id=id, classes=classes)
        self._commands_list: list[SlashCommand] = []

    def compose(self) -> ComposeResult:
        yield OptionList(id="slash-options")

    def on_mount(self) -> None:
        """Populate options on mount."""
        self._rebuild_options()

    def watch_slash_commands(self) -> None:
        """Rebuild options when commands change."""
        if not self.is_mounted:
            return
        self._rebuild_options()

    def _rebuild_options(self) -> None:
        """Build the options list from commands."""
        try:
            option_list = self.query_one("#slash-options", OptionList)
        except Exception:
            return
        option_list.clear_options()
        self._commands_list = list(self.slash_commands)

        for cmd in self._commands_list:
            option_text = f"/{cmd.command}  {cmd.help}"
            option_list.add_option(Option(option_text, id=cmd.command))

        if self._commands_list:
            option_list.highlighted = 0

    def action_cursor_up(self) -> None:
        """Move cursor up."""
        self.query_one("#slash-options", OptionList).action_cursor_up()

    def action_cursor_down(self) -> None:
        """Move cursor down."""
        self.query_one("#slash-options", OptionList).action_cursor_down()

    def action_select(self) -> None:
        """Select the highlighted option."""
        option_list = self.query_one("#slash-options", OptionList)
        if option_list.highlighted is not None:
            idx = option_list.highlighted
            if 0 <= idx < len(self._commands_list):
                cmd = self._commands_list[idx]
                self.post_message(self.Completed(command=cmd.command))

    def action_dismiss(self) -> None:
        """Dismiss the autocomplete widget."""
        self.post_message(self.Dismissed())
