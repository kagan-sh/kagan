"""ANSI escape sequence stripper.

Provides efficient regex-based stripping of ANSI escape codes from terminal output.
"""

from __future__ import annotations

import re

# Combined pattern for all ANSI escape sequences:
# - CSI sequences: ESC [ ... final_byte (colors, cursor, etc.)
# - OSC sequences: ESC ] ... BEL (terminal title, etc.)
# - Simple escapes: ESC followed by single char
ANSI_ESCAPE = re.compile(r"\x1B(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*\x07|[@-Z\\^_-])")


def strip_ansi(text: str) -> str:
    """Remove all ANSI escape sequences from text.

    Args:
        text: Input text potentially containing ANSI codes.

    Returns:
        Clean text with all escape sequences removed.
    """
    if not text:
        return ""
    return ANSI_ESCAPE.sub("", text)


def clean_terminal_output(text: str) -> str:
    """Process terminal output to produce clean readable text.

    This is an alias for strip_ansi() that maintains backward compatibility
    with code expecting the old terminal emulator interface.

    For simple ANSI stripping, this works well. For cases requiring
    full terminal emulation (cursor movement, line clearing), consider
    using a proper terminal emulator library.

    Args:
        text: Raw terminal output with control characters.

    Returns:
        Clean text with escape sequences removed.
    """
    return strip_ansi(text)
