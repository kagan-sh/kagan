"""Shared TUI utility helpers.

Consolidates: agent_exit, animation, queries, diff, clipboard, path utilities.
"""

from __future__ import annotations

import re
from contextlib import suppress
from pathlib import Path
from typing import TYPE_CHECKING, TypeVar

import pyperclip
from rich.markup import escape
from textual.css.query import NoMatches
from textual.widget import Widget

if TYPE_CHECKING:
    from textual.app import App

T = TypeVar("T", bound=Widget)

# ---------------------------------------------------------------------------
# Agent exit utilities
# ---------------------------------------------------------------------------

_EXIT_CODE_PATTERN = re.compile(r"Agent exited with code (?P<code>-?\d+)")
SIGTERM_EXIT_CODE = -15


def parse_agent_exit_code(message: str) -> int | None:
    """Extract process exit code from an agent failure message."""
    match = _EXIT_CODE_PATTERN.search(message)
    if match is None:
        return None
    try:
        return int(match.group("code"))
    except ValueError:
        return None


def is_graceful_agent_termination(message: str) -> bool:
    """Return True when failure message represents expected SIGTERM cancellation."""
    return parse_agent_exit_code(message) == SIGTERM_EXIT_CODE


# ---------------------------------------------------------------------------
# Animation constants
# ---------------------------------------------------------------------------

WAVE_FRAMES = [
    "ᘚᘚᘚᘚ",
    "ᘛᘚᘚᘚ",
    "ᘛᘛᘚᘚ",
    "ᘛᘛᘛᘚ",
    "ᘛᘛᘛᘛ",
    "ᘚᘛᘛᘛ",
    "ᘚᘚᘛᘛ",
    "ᘚᘚᘚᘛ",
]

WAVE_INTERVAL_MS = 100

# ---------------------------------------------------------------------------
# Widget query utilities
# ---------------------------------------------------------------------------


def safe_query_one(
    parent: Widget,
    selector: str,
    widget_class: type[T],
    default: T | None = None,
) -> T | None:
    """Query widget safely, returning default on NoMatches."""
    with suppress(NoMatches):
        return parent.query_one(selector, widget_class)
    return default


# ---------------------------------------------------------------------------
# Diff colorization
# ---------------------------------------------------------------------------


def colorize_diff_line(line: str) -> str:
    """Colorize a single diff line with Rich markup."""
    escaped = escape(line)
    if line.startswith("+") and not line.startswith("+++"):
        return f"[green]{escaped}[/green]"
    if line.startswith("-") and not line.startswith("---"):
        return f"[red]{escaped}[/red]"
    if line.startswith("@@"):
        return f"[cyan]{escaped}[/cyan]"
    return escaped


def colorize_diff(content: str) -> str:
    """Colorize a full diff string with Rich markup."""
    return "\n".join(colorize_diff_line(line) for line in content.split("\n"))


# ---------------------------------------------------------------------------
# Clipboard
# ---------------------------------------------------------------------------


def copy_with_notification(app: App, text: str, label: str = "Content") -> bool:
    """Copy text to clipboard and show toast notification."""
    if not text or not text.strip():
        app.notify("Nothing to copy", severity="warning")
        return False

    try:
        pyperclip.copy(text)

        preview = text[:50].replace("\n", " ")
        if len(text) > 50:
            preview += "..."
        app.notify(f"{label} copied to clipboard")
        return True
    except pyperclip.PyperclipException as e:
        app.notify(f"Copy failed: {e}", severity="error")
        return False


# ---------------------------------------------------------------------------
# Path display
# ---------------------------------------------------------------------------


def truncate_path(path: str, max_width: int = 40) -> str:
    """Truncate path preserving final directory name."""
    home = str(Path.home())
    if path.startswith(home):
        path = "~" + path[len(home) :]

    if len(path) <= max_width:
        return path

    parts = Path(path).parts
    if not parts:
        return path

    final = parts[-1]
    prefix = parts[0] if parts else ""

    truncated = f"{prefix}/.../{final}"

    if len(truncated) > max_width:
        truncated = f".../{final}"

    if len(truncated) > max_width:
        available = max_width - 4
        truncated = f".../{final[:available]}..."

    return truncated
