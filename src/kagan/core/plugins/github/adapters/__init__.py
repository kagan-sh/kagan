"""GitHub plugin adapters for core and gh CLI."""

from __future__ import annotations

from kagan.core.plugins.github.adapters.core_gateway import AppContextCoreGateway
from kagan.core.plugins.github.adapters.gh_cli_client import GhCliClientAdapter

__all__ = ["AppContextCoreGateway", "GhCliClientAdapter"]
