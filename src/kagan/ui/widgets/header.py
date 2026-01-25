"""Header widget for Kagan TUI."""

from contextlib import suppress

from textual.app import ComposeResult
from textual.css.query import NoMatches
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Label


class KaganHeader(Widget):
    """Header widget displaying app title and statistics."""

    ticket_count: reactive[int] = reactive(0)

    def __init__(self, ticket_count: int = 0, **kwargs) -> None:
        super().__init__(**kwargs)
        self.ticket_count = ticket_count

    def compose(self) -> ComposeResult:
        yield Label("KAGAN", classes="header-title")
        yield Label("? Help  q Quit", classes="header-help")
        yield Label(f"Tickets: {self.ticket_count}", classes="header-stats")

    def watch_ticket_count(self, count: int) -> None:
        with suppress(NoMatches):
            self.query_one(".header-stats", Label).update(f"Tickets: {count}")

    def update_count(self, count: int) -> None:
        self.ticket_count = count
