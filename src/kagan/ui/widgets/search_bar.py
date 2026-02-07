"""SearchBar widget for filtering tasks."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from textual.message import Message
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Input

from kagan.ui.utils import safe_query_one

if TYPE_CHECKING:
    from textual.app import ComposeResult


class SearchBar(Widget):
    """A search bar widget for filtering tasks on the Kanban board."""

    # Start hidden and non-focusable
    can_focus = False

    search_query: reactive[str] = reactive("")
    is_visible: reactive[bool] = reactive(False)

    @dataclass
    class QueryChanged(Message):
        """Posted when the search query changes."""

        query: str

    def compose(self) -> ComposeResult:
        """Compose the search bar layout."""
        yield Input(placeholder="Search tasks...", id="search-input")

    def on_mount(self) -> None:
        """Disable focus on the input when mounted (hidden by default)."""
        if inp := safe_query_one(self, "#search-input", Input):
            inp.can_focus = False

    def on_input_changed(self, event: Input.Changed) -> None:
        """Handle input text changes."""
        self.search_query = event.value
        self.post_message(self.QueryChanged(self.search_query))

    def watch_is_visible(self, is_visible: bool) -> None:
        """Toggle visibility CSS class."""
        if is_visible:
            self.add_class("visible")
        else:
            self.remove_class("visible")

    def show(self) -> None:
        """Show the search bar and focus the input."""
        self.is_visible = True
        if inp := safe_query_one(self, "#search-input", Input):
            inp.can_focus = True
            inp.focus()

    def hide(self) -> None:
        """Hide the search bar and clear the query."""
        self.is_visible = False
        if inp := safe_query_one(self, "#search-input", Input):
            inp.can_focus = False
        self.clear()

    def clear(self) -> None:
        """Clear the search query."""
        self.search_query = ""
        if inp := safe_query_one(self, "#search-input", Input):
            inp.value = ""
