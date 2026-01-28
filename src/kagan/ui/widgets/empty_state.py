"""Empty state widget for planner screen."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.containers import Center, Vertical
from textual.widget import Widget
from textual.widgets import Static

from kagan.constants import BOX_DRAWING

if TYPE_CHECKING:
    from textual.app import ComposeResult


class EmptyState(Widget):
    """Empty state widget showing tutorial content for planner screen."""

    DEFAULT_CLASSES = "empty-state"

    def compose(self) -> ComposeResult:
        """Compose the empty state layout."""
        with Center():
            with Vertical(classes="empty-state-card"):
                yield Static("Getting Started", classes="empty-card-title")

                # How it works
                yield Static("How it works:", classes="empty-card-section")
                yield Static(
                    f"  {BOX_DRAWING['BULLET']} Describe your feature below",
                    classes="card-item-compact",
                )
                yield Static(
                    f"  {BOX_DRAWING['BULLET']} AI analyzes & asks questions",
                    classes="card-item-compact",
                )
                yield Static(
                    f"  {BOX_DRAWING['BULLET']} Generates tickets for review",
                    classes="card-item-compact",
                )

                # Tips
                yield Static("Tips:", classes="empty-card-section")
                yield Static(
                    f"  {BOX_DRAWING['BULLET']} Be specific about requirements",
                    classes="card-item-compact",
                )
                yield Static(
                    f"  {BOX_DRAWING['BULLET']} Mention tech stack/constraints",
                    classes="card-item-compact",
                )
                yield Static(
                    f"  {BOX_DRAWING['BULLET']} Press Esc to return to board",
                    classes="card-item-compact",
                )
