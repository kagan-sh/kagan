"""Keybinding hint generation for the Kanban screen."""

from __future__ import annotations

from kagan.database.models import TicketStatus, TicketType


def build_keybinding_hints(
    status: TicketStatus | None,
    ticket_type: TicketType | None,
) -> list[tuple[str, str]]:
    """Build context-sensitive keybinding hints based on ticket state.

    Args:
        status: Current ticket status, or None if no ticket selected.
        ticket_type: Ticket type (AUTO/PAIR), or None if no ticket selected.

    Returns:
        List of (key, description) tuples for display.
    """
    # No ticket selected - show general actions
    if status is None:
        return [
            ("n", "new ticket"),
            ("N", "new AUTO"),
            ("ctrl+p", "planner"),
            ("/", "search"),
            ("g", "more actions..."),
        ]

    if status == TicketStatus.BACKLOG:
        return [
            ("Enter", "start"),
            ("e", "edit"),
            ("v", "details"),
            ("x", "delete"),
            ("g", "more..."),
        ]

    if status == TicketStatus.IN_PROGRESS:
        if ticket_type == TicketType.AUTO:
            return [
                ("w", "watch agent"),
                ("s", "stop agent"),
                ("v", "details"),
                ("p", "peek status"),
                ("g", "more..."),
            ]
        return [
            ("a", "open session"),
            ("v", "details"),
            ("l", "advance"),
            ("g", "more..."),
        ]

    if status == TicketStatus.REVIEW:
        return [
            ("r", "AI review"),
            ("D", "view diff"),
            ("m", "merge"),
            ("h", "move back"),
            ("g", "more..."),
        ]

    if status == TicketStatus.DONE:
        return [
            ("v", "view details"),
            ("h", "reopen"),
            ("g", "more..."),
        ]

    return []
