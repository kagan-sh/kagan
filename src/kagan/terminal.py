"""Terminal capability detection utilities."""

from __future__ import annotations

import os

# Terminals that are KNOWN to NOT support truecolor
# Check these FIRST, before trusting COLORTERM (which may be incorrectly set)
_NO_TRUECOLOR_TERMINALS = {
    "apple_terminal",  # macOS Terminal.app - only supports 256 colors
}

# Terminals that are KNOWN to support truecolor
_TRUECOLOR_TERMINALS = {
    "iterm.app",
    "vscode",
    "hyper",
    "alacritty",
    "kitty",
    "wezterm",
    "ghostty",
    "warp",
    "tabby",
    "rio",
    "contour",
}


def supports_truecolor() -> bool:
    """Check if the terminal supports truecolor (24-bit colors).

    Detection logic (in order of priority):
    1. If TEXTUAL_COLOR_SYSTEM is set to 'truecolor', return True (explicit user override)
    2. If TERM_PROGRAM is known to NOT support truecolor (Apple_Terminal), return False
    3. If TERM_PROGRAM is known to support truecolor (iTerm.app, vscode, etc.), return True
    4. If COLORTERM is set to 'truecolor' or '24bit', return True
    5. If WT_SESSION is set (Windows Terminal), return True
    6. Otherwise, return False

    Note: We check TERM_PROGRAM before COLORTERM because some shell configs
    incorrectly set COLORTERM=truecolor even in terminals that don't support it.

    Returns:
        True if truecolor is likely supported, False otherwise.
    """
    # Check for explicit user override via Textual's environment variable
    # This takes highest priority - user knows best
    textual_color = os.environ.get("TEXTUAL_COLOR_SYSTEM", "").lower()
    if textual_color == "truecolor":
        return True

    # Check TERM_PROGRAM for known terminals (both supported and unsupported)
    # We check this BEFORE COLORTERM because COLORTERM may be incorrectly set
    term_program = os.environ.get("TERM_PROGRAM", "").lower()

    # First, check if it's a terminal we KNOW doesn't support truecolor
    if term_program in _NO_TRUECOLOR_TERMINALS:
        return False

    # Then check if it's a terminal we KNOW supports truecolor
    if term_program in _TRUECOLOR_TERMINALS:
        return True

    # Now check COLORTERM - but only if TERM_PROGRAM didn't give us a definitive answer
    colorterm = os.environ.get("COLORTERM", "").lower()
    if colorterm in ("truecolor", "24bit"):
        return True

    # Check WT_SESSION for Windows Terminal
    return bool(os.environ.get("WT_SESSION"))


def get_terminal_name() -> str:
    """Get a human-readable name for the current terminal.

    Returns:
        Terminal name or 'Unknown terminal'.
    """
    # Check TERM_PROGRAM first (most informative)
    term_program = os.environ.get("TERM_PROGRAM", "")
    if term_program:
        # Format nicely for common terminals
        nice_names = {
            "Apple_Terminal": "macOS Terminal.app",
            "iTerm.app": "iTerm2",
            "vscode": "VS Code Terminal",
        }
        return nice_names.get(term_program, term_program)

    # Check for Windows Terminal
    if os.environ.get("WT_SESSION"):
        return "Windows Terminal"

    # Fall back to TERM
    term = os.environ.get("TERM", "")
    if term:
        return term

    return "Unknown terminal"
