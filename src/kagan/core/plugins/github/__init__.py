"""Bundled official GitHub plugin scaffold exports."""

from __future__ import annotations

from kagan.core.plugins.github.contract import (
    GITHUB_CANONICAL_METHODS,
    GITHUB_CANONICAL_METHODS_SCOPE,
    GITHUB_CAPABILITY,
    GITHUB_CONTRACT_PROBE_METHOD,
    GITHUB_CONTRACT_VERSION,
    GITHUB_METHOD_CONNECT_REPO,
    GITHUB_PLUGIN_ID,
    RESERVED_GITHUB_CAPABILITY,
)
from kagan.core.plugins.github.plugin import GitHubPlugin, register_github_plugin

__all__ = [
    "GITHUB_CANONICAL_METHODS",
    "GITHUB_CANONICAL_METHODS_SCOPE",
    "GITHUB_CAPABILITY",
    "GITHUB_CONTRACT_PROBE_METHOD",
    "GITHUB_CONTRACT_VERSION",
    "GITHUB_METHOD_CONNECT_REPO",
    "GITHUB_PLUGIN_ID",
    "RESERVED_GITHUB_CAPABILITY",
    "GitHubPlugin",
    "register_github_plugin",
]
