"""Modal displayed when another Kagan instance is running in a repository."""

from __future__ import annotations

from typing import TYPE_CHECKING

from textual.binding import Binding
from textual.containers import Center, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Static

from kagan.tui.ui.user_messages import instance_lock_copy

if TYPE_CHECKING:
    from textual.app import ComposeResult

    from kagan.core.instance_lock import LockInfo


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
        copy = instance_lock_copy(is_startup=self._is_startup)

        with Vertical(id="instance-locked-container"):
            yield Static(f"⚠ {copy.title}", id="instance-locked-title")
            yield Static(copy.message, id="instance-locked-message")

            if self._lock_info:
                info_parts = [f"PID: {self._lock_info.pid}"]
                info_parts.append(f"Host: {self._lock_info.hostname}")
                if self._lock_info.repo_path:
                    # Show just the last part of the path for readability
                    from pathlib import Path

                    repo_name = Path(self._lock_info.repo_path).name
                    info_parts.append(f"Repo: {repo_name}")
                yield Static("  •  ".join(info_parts), id="instance-locked-info")

            yield Static(copy.note, id="instance-locked-note")

            with Center():
                yield Button(
                    copy.button_label,
                    variant=copy.button_variant,
                    id="instance-locked-quit",
                )

    def action_dismiss_modal(self) -> None:
        """Dismiss the modal - quit app at startup, just close otherwise."""
        if self._is_startup:
            self.app.exit()
        else:
            self.dismiss()

    def on_button_pressed(self) -> None:
        """Handle button press."""
        self.action_dismiss_modal()
