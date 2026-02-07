"""Cross-platform command-line lexical helpers.

Uses `mslex` on Windows for cmd.exe-compatible parsing/quoting and
`shlex` elsewhere.  Both libraries expose an identical API surface
(split, quote, join), so we pick one at call time based on
``platform.system()``.
"""

from __future__ import annotations

import platform
import shlex
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from types import ModuleType


def _lex() -> ModuleType:
    """Return the appropriate lexer module for the current platform."""
    if platform.system() == "Windows":
        import mslex

        return mslex
    return shlex


def split_command(command: str) -> list[str]:
    """Split a command string into argv using OS-appropriate rules."""
    return _lex().split(command)


def quote_arg(value: str) -> str:
    """Quote a single argument safely for the active platform shell."""
    return _lex().quote(value)


def join_args(args: list[str]) -> str:
    """Join argv into a shell command fragment for the active platform."""
    return _lex().join(args)
