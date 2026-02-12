"""Planner command palette provider."""

from __future__ import annotations

from dataclasses import dataclass
from functools import partial

from textual.command import DiscoveryHit, Hit, Hits, Provider

from kagan.tui.ui.screens.command_actions import run_screen_action, screen_allows_action


@dataclass(frozen=True, slots=True)
class ScreenAction:
    command: str
    help: str
    action: str


PLANNER_ACTIONS: tuple[ScreenAction, ...] = (
    ScreenAction("planner enhance", "Refine prompt", "refine"),
    ScreenAction("planner back to board", "Return to board", "to_board"),
    ScreenAction("planner stop", "Stop planner or clear input", "cancel"),
)


class PlannerCommandProvider(Provider):
    """Command palette provider for planner actions."""

    async def search(self, query: str) -> Hits:
        screen = self.screen
        matcher = self.matcher(query)
        for item in PLANNER_ACTIONS:
            if not screen_allows_action(screen, item.action):
                continue
            score = matcher.match(item.command)
            if score <= 0:
                continue
            yield Hit(
                score,
                matcher.highlight(item.command),
                partial(run_screen_action, screen, item.action),
                help=item.help,
            )

    async def discover(self) -> Hits:
        screen = self.screen
        for item in PLANNER_ACTIONS:
            if not screen_allows_action(screen, item.action):
                continue
            yield DiscoveryHit(
                item.command,
                partial(run_screen_action, screen, item.action),
                help=item.help,
            )
