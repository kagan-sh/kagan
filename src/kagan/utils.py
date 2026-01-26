"""Utility functions for Kagan."""

from __future__ import annotations

from kagan.database.models import TicketStatus  # noqa: TC001 - used at runtime


def truncate(text: str, max_length: int, suffix: str = "...") -> str:
    """Truncate text to max_length, adding suffix if truncated."""
    if len(text) <= max_length:
        return text
    return text[: max_length - len(suffix)] + suffix


def get_status_style(status: str) -> str:
    """Get Rich style for status string."""
    return "green" if status == "completed" else "yellow"


def column_id(status: TicketStatus) -> str:
    """Generate column element ID."""
    return f"column-{status.value.lower()}"


def card_id(ticket_id: str) -> str:
    """Generate card element ID."""
    return f"card-{ticket_id}"


def log_exception(e: Exception, context: str = "") -> None:
    """Log exception with traceback to textual log."""
    import traceback

    from textual import log

    tb = traceback.format_exc()
    prefix = f"{context}: " if context else ""
    log.error(f"{prefix}{e}")
    log.error(f"Traceback:\n{tb}")
