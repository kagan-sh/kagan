"""Modal displayed when another Kagan instance is running in a repository."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.binding import Binding
from textual.containers import Center, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Static

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.instance_lock import LockInfo


class InstanceLockedModal(ModalScreen[None]):
    """Modal shown when repository is locked by another instance.

    Args:
        lock_info: Information about the process holding the lock.
        is_startup: If True, this is shown at app startup (quit app on dismiss).
                   If False, this is shown during repo switch (just dismiss modal).
    """

    BINDINGS = [
        Binding("enter", "dismiss_modal", "OK", priority=True),
        Binding("escape", "dismiss_modal", "Cancel"),
    ]

    def __init__(self, lock_info: LockInfo | None = None, *, is_startup: bool = True) -> None:
        super().__init__()
        self._lock_info = lock_info
        self._is_startup = is_startup

    def compose(self) -> ComposeResult:
        with Vertical(id="instance-locked-container"):
            yield Static("⚠ Repository Locked", id="instance-locked-title")

            if self._is_startup:
                message = "Another Kagan instance is running in this repository."
            else:
                message = "Cannot switch to this repository — it's locked by another instance."

            yield Static(message, id="instance-locked-message")

            if self._lock_info:
                info_parts = [f"PID: {self._lock_info.pid}"]
                info_parts.append(f"Host: {self._lock_info.hostname}")
                if self._lock_info.repo_path:
                    # Show just the last part of the path for readability
                    from pathlib import Path

                    repo_name = Path(self._lock_info.repo_path).name
                    info_parts.append(f"Repo: {repo_name}")
                yield Static("  •  ".join(info_parts), id="instance-locked-info")

            if self._is_startup:
                note = "Only one instance can run per repository to prevent conflicts."
                button_label = "Quit"
                button_variant = "error"
            else:
                note = "Close the other instance first, or continue working in your current repo."
                button_label = "OK"
                button_variant = "primary"

            yield Static(note, id="instance-locked-note")

            with Center():
                yield Button(button_label, variant=button_variant, id="instance-locked-quit")

    def action_dismiss_modal(self) -> None:
        """Dismiss the modal - quit app at startup, just close otherwise."""
        if self._is_startup:
            self.app.exit()
        else:
            self.dismiss()

    def on_button_pressed(self) -> None:
        """Handle button press."""
        self.action_dismiss_modal()
