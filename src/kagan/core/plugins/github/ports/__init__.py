"""GitHub plugin integration ports."""

from __future__ import annotations

from kagan.core.plugins.github.ports.core_gateway import GitHubCoreGateway
from kagan.core.plugins.github.ports.gh_client import GitHubClient

__all__ = ["GitHubClient", "GitHubCoreGateway"]
