"""Pure formatting functions for ticket cards."""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from kagan.database.models import Ticket, TicketStatus


def truncate_text(text: str, max_length: int) -> str:
    """Truncate text if too long.

    Args:
        text: Text to truncate
        max_length: Maximum length including ellipsis

    Returns:
        Truncated text with ellipsis if needed
    """
    if len(text) <= max_length:
        return text
    return text[: max_length - 3] + "..."


def wrap_title(title: str, line_width: int) -> list[str]:
    """Wrap title into multiple lines, respecting word boundaries.

    Args:
        title: Title text to wrap
        line_width: Maximum characters per line

    Returns:
        List of wrapped lines (max 2 lines)
    """
    if len(title) <= line_width:
        return [title]

    # Try to break at word boundary
    words = title.split()
    lines: list[str] = []
    current_line = ""

    for word in words:
        test_line = f"{current_line} {word}".strip() if current_line else word
        if len(test_line) <= line_width:
            current_line = test_line
        else:
            if current_line:
                lines.append(current_line)
            # Handle very long words
            current_line = word[: line_width - 3] + "..." if len(word) > line_width else word

        # Limit to 2 lines
        if len(lines) >= 2:
            break

    if current_line and len(lines) < 2:
        # Truncate final line if needed
        if len(current_line) > line_width:
            current_line = current_line[: line_width - 3] + "..."
        lines.append(current_line)

    return lines if lines else [title[: line_width - 3] + "..."]


def format_progress_bar(iteration_info: str) -> str:
    """Format iteration info as progress bar visualization.

    Args:
        iteration_info: String like "Iter 3/10"

    Returns:
        Formatted progress bar like "[■■■□□□□□□□] 3/10"
    """
    if not iteration_info:
        return ""

    # Parse "Iter X/Y" format
    try:
        parts = iteration_info.split()
        if len(parts) >= 2 and "/" in parts[1]:
            current, total = parts[1].split("/")
            current_int = int(current)
            total_int = int(total)
            filled = int((current_int / total_int) * 10) if total_int > 0 else 0
            empty = 10 - filled
            bar = "■" * filled + "□" * empty
            return f"[{bar}] {current}/{total}"
    except (ValueError, IndexError):
        pass

    return iteration_info


def get_review_badge(ticket: Ticket | None) -> str:
    """Get review badge icon.

    Args:
        ticket: Ticket to get badge for

    Returns:
        Badge emoji: ⏳ pending, ✓ passed, ✗ failed, ⚠ blocked
    """
    if ticket is None:
        return "⏳"
    if ticket.merge_failed:
        return "⚠"
    if ticket.checks_passed is True:
        return "✓"
    if ticket.checks_passed is False:
        return "✗"
    return "⏳"


def format_checks_status(ticket: Ticket | None) -> str:
    """Format checks status with icon and text.

    Args:
        ticket: Ticket to format status for

    Returns:
        Status string like "✓ Review approved"
    """
    if ticket is None:
        return "⏳ Review pending"
    if ticket.merge_failed:
        error = ticket.merge_error or "unknown error"
        return f"⚠ Merge failed: {error[:40]}"
    if ticket.checks_passed is True:
        return "✓ Review approved"
    if ticket.checks_passed is False:
        return "✗ Review rejected"
    return "⏳ Review pending"


def format_review_status(ticket: Ticket | None, merge_readiness: str) -> str:
    """Format consolidated review status with merge readiness.

    Args:
        ticket: Ticket to format status for
        merge_readiness: Merge readiness state (ready/blocked/risk)

    Returns:
        Status string like "✓ Review approved · Ready to merge"
    """
    if ticket is None:
        return "⏳ Review pending"

    # Merge blocked - show error inline
    if ticket.merge_failed:
        error = ticket.merge_error or "unknown error"
        return f"⚠ Merge blocked: {error[:35]}"

    # Review passed/failed with readiness indicator
    readiness = merge_readiness or "risk"
    if ticket.checks_passed is True:
        if readiness == "ready":
            return "✓ Review approved · Ready to merge"
        elif readiness == "blocked":
            return "✓ Review approved · Blocked"
        else:
            return "✓ Review approved · At risk"
    elif ticket.checks_passed is False:
        return "✗ Review rejected"
    else:
        return "⏳ Review pending"


def get_readiness_badge(ticket: Ticket | None, merge_readiness: str, status: TicketStatus) -> str:
    """Get merge readiness badge for REVIEW tickets.

    Args:
        ticket: Ticket to get badge for
        merge_readiness: Merge readiness state
        status: Ticket status

    Returns:
        Badge like "🟢 SAFE" or empty string for non-REVIEW
    """
    from kagan.database.models import TicketStatus

    if ticket is None or status != TicketStatus.REVIEW:
        return ""
    readiness = merge_readiness or "risk"
    if readiness == "ready":
        return "🟢 SAFE"
    if readiness == "blocked":
        return "🔴 BLOCKED"
    return "🟡 RISK"
