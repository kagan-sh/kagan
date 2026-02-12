"""Terminal backend helpers."""

from kagan.tui.terminals.installer import (
    check_terminal_installed,
    get_manual_install_fallback,
    install_terminal,
)

__all__ = [
    "check_terminal_installed",
    "get_manual_install_fallback",
    "install_terminal",
]
