"""Clipboard utilities for copy-to-clipboard functionality."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pyperclip

if TYPE_CHECKING:
    from textual.app import App


def copy_with_notification(app: App, text: str, label: str = "Content") -> bool:
    """Copy text to clipboard and show toast notification.

    Args:
        app: The Textual app instance for showing notifications
        text: Text to copy to clipboard
        label: Description for notification (e.g., "Diff", "Response")

    Returns:
        True if copy succeeded, False otherwise
    """
    if not text or not text.strip():
        app.notify("Nothing to copy", severity="warning")
        return False

    try:
        pyperclip.copy(text)
        # Truncate preview for notification
        preview = text[:50].replace("\n", " ")
        if len(text) > 50:
            preview += "..."
        app.notify(f"{label} copied to clipboard")
        return True
    except pyperclip.PyperclipException as e:
        app.notify(f"Copy failed: {e}", severity="error")
        return False
