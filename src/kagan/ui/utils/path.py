"""Path utilities for display formatting."""

from __future__ import annotations

from pathlib import Path


def truncate_path(path: str, max_width: int = 40) -> str:
    """Truncate path preserving final directory name.

    Replaces home directory with ~ and uses ... to show truncation
    while preserving the final directory component.

    Args:
        path: The full path to truncate
        max_width: Maximum length of the result

    Returns:
        Truncated path with leading ellipsis if needed

    Examples:
        /Users/dev/workspace/projects/my-project -> ~/workspace/.../my-project
        /Users/dev/short -> ~/short
    """

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
