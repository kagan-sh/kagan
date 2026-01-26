"""AgentGrid widget for selecting agents."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Grid
from textual.reactive import reactive

from kagan.ui.widgets.agent_card import AgentCard

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.data.builtin_agents import BuiltinAgent


class AgentGrid(Grid, can_focus=True):
    """Grid of selectable agent cards."""

    BINDINGS = [
        ("left", "cursor_left", "Previous"),
        ("right", "cursor_right", "Next"),
    ]

    highlighted: reactive[int] = reactive(0)

    def __init__(self, agents: list[BuiltinAgent], **kwargs) -> None:
        super().__init__(**kwargs)
        self.agents = agents

    def compose(self) -> ComposeResult:
        """Compose the grid of agent cards."""
        for agent in self.agents:
            yield AgentCard(agent, id=f"agent-{agent.config.short_name}")

    def on_mount(self) -> None:
        """Update selection on mount."""
        self._update_selection()

    def watch_highlighted(self, highlighted: int) -> None:
        """Update card selection when highlighted changes."""
        self._update_selection()

    def _update_selection(self) -> None:
        """Update which card is selected."""
        for i, card in enumerate(self.query(AgentCard)):
            card.selected = i == self.highlighted

    def action_cursor_left(self) -> None:
        """Move cursor left."""
        self.highlighted = max(0, self.highlighted - 1)

    def action_cursor_right(self) -> None:
        """Move cursor right."""
        self.highlighted = min(len(self.agents) - 1, self.highlighted + 1)

    def get_selected_agent(self) -> BuiltinAgent:
        """Get the currently selected agent."""
        return self.agents[self.highlighted]
