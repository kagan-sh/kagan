"""Pure formatting functions for task cards."""

from __future__ import annotations


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

            current_line = word[: line_width - 3] + "..." if len(word) > line_width else word

        if len(lines) >= 2:
            break

    if current_line and len(lines) < 2:
        if len(current_line) > line_width:
            current_line = current_line[: line_width - 3] + "..."
        lines.append(current_line)

    return lines if lines else [title[: line_width - 3] + "..."]
