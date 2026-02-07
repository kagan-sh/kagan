"""Shared diff colorization utilities."""

from __future__ import annotations


def colorize_diff_line(line: str) -> str:
    """Colorize a single diff line with Rich markup."""
    if line.startswith("+") and not line.startswith("+++"):
        return f"[green]{line}[/green]"
    if line.startswith("-") and not line.startswith("---"):
        return f"[red]{line}[/red]"
    if line.startswith("@@"):
        return f"[cyan]{line}[/cyan]"
    return line


def colorize_diff(content: str) -> str:
    """Colorize a full diff string with Rich markup."""
    return "\n".join(colorize_diff_line(line) for line in content.split("\n"))
