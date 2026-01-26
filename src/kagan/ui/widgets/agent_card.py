"""AgentCard widget for displaying a selectable agent."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Vertical
from textual.reactive import reactive
from textual.widgets import Label, Static

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.data.builtin_agents import BuiltinAgent


class AgentCard(Static, can_focus=True):
    """Selectable card representing an agent."""

    selected: reactive[bool] = reactive(False)

    def __init__(self, agent: BuiltinAgent, **kwargs) -> None:
        super().__init__(**kwargs)
        self.agent = agent

    def compose(self) -> ComposeResult:
        """Compose the card layout."""
        with Vertical():
            yield Label(self.agent.config.name, classes="card-name")
            yield Label("─" * 12, classes="card-separator")
            yield Label(self.agent.author, classes="card-author")
            yield Label(self.agent.description, classes="card-desc")

    def watch_selected(self, selected: bool) -> None:
        """Update styling when selected state changes."""
        self.set_class(selected, "-selected")
