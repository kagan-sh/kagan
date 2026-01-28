"""Kagan: AI-powered Kanban TUI for autonomous development workflows."""

from importlib.metadata import PackageNotFoundError, version

# Re-export OS detection utilities
from kagan.config import CURRENT_OS, get_os_value

try:
    __version__ = version("kagan")
except PackageNotFoundError:
    __version__ = "dev"

__all__ = ["CURRENT_OS", "get_os_value"]
