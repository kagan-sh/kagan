"""StatusBar widget for displaying agent status and contextual hints."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Static

from kagan.ui.utils import safe_query_one
from kagan.ui.utils.animation import WAVE_FRAMES, WAVE_INTERVAL_MS

if TYPE_CHECKING:
    from textual.app import ComposeResult


class StatusBar(Widget):
    """Status bar showing agent state and contextual hints."""

    status: reactive[str] = reactive("waiting")
    hint: reactive[str] = reactive("Initializing agent...")

    STATUS_INDICATORS = {
        "ready": "●",
        "thinking": WAVE_FRAMES[0],  # Will be animated
        "refining": WAVE_FRAMES[0],  # Will be animated
        "error": "✗",
        "waiting": "○",
        "initializing": WAVE_FRAMES[0],  # Will be animated
    }

    def __init__(self, **kwargs) -> None:
        if "id" not in kwargs:
            kwargs["id"] = "planner-status-bar"
        super().__init__(**kwargs)
        self._frame_index = 0
        self._timer = None

    def compose(self) -> ComposeResult:
        yield Static("", classes="status-left")
        yield Static("", classes="status-right")

    def on_mount(self) -> None:
        """Initialize status display on mount."""
        self._update_display()

    def watch_status(self, status: str) -> None:
        """Update display and manage animation when status changes."""
        if status in ("thinking", "refining", "initializing"):
            self._start_animation()
        else:
            self._stop_animation()
        self._update_display()

    def watch_hint(self, _hint: str) -> None:
        """Update display when hint changes."""
        self._update_display()

    def _start_animation(self) -> None:
        """Start the wave animation for thinking state."""
        if self._timer is None:
            self._frame_index = 0
            self._timer = self.set_interval(WAVE_INTERVAL_MS / 1000, self._next_frame, pause=False)

    def _stop_animation(self) -> None:
        """Stop the wave animation."""
        if self._timer is not None:
            self._timer.stop()
            self._timer = None
            self._frame_index = 0

    def _next_frame(self) -> None:
        """Advance to the next animation frame."""
        self._frame_index = (self._frame_index + 1) % len(WAVE_FRAMES)
        self._update_display()

    def update_status(self, status: str, hint: str = "") -> None:
        """Update status and hint text.

        Args:
            status: Status state ("ready", "thinking", "error", "waiting")
            hint: Optional contextual hint text
        """
        self.status = status
        if hint:
            self.hint = hint

    def _update_display(self) -> None:
        """Update the status bar display."""
        if self.status in ("thinking", "refining", "initializing"):
            symbol = WAVE_FRAMES[self._frame_index]
        else:
            symbol = self.STATUS_INDICATORS.get(self.status, "○")
        status_text = f"{symbol} {self.status.capitalize()}"

        if left := safe_query_one(self, ".status-left", Static):
            left.update(status_text)
        if right := safe_query_one(self, ".status-right", Static):
            right.update(self.hint)
