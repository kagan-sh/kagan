"""Numeric limits and timeouts - no circular dependencies."""

from __future__ import annotations

import os


def _is_debug_build() -> bool:
    """Check if this is a debug/beta build.

    Debug mode is enabled when:
    1. KAGAN_DEBUG env var is set to "1" or "true" (explicit override)
    2. Version contains "b" (beta), "a" (alpha), "rc" (release candidate), or "dev"
    3. Version is "dev" (local development)

    Production releases (e.g., "0.3.0") have debug disabled by default.
    """
    env_debug = os.environ.get("KAGAN_DEBUG", "").lower()
    if env_debug in ("1", "true"):
        return True
    if env_debug in ("0", "false"):
        return False

    try:
        from importlib.metadata import version

        pkg_version = version("kagan")
    except Exception:
        pkg_version = "dev"

    # Check for pre-release indicators (PEP 440 format: 0.3.0b1, 0.3.0a1, 0.3.0rc1)
    # Also check semantic versioning format: 0.3.0-beta.1, 0.3.0-alpha.1
    version_lower = pkg_version.lower()
    return any(indicator in version_lower for indicator in ("dev", "a", "b", "rc", "alpha", "beta"))


DEBUG_BUILD: bool = _is_debug_build()
"""True for beta/dev builds, False for production releases."""


AGENT_TIMEOUT = 30.0
AGENT_TIMEOUT_LONG = 60.0
SHUTDOWN_TIMEOUT = 5.0


RESPONSE_BUFFER = 10000
MESSAGE_BUFFER = 500
SUBPROCESS_LIMIT = 10 * 1024 * 1024
SCRATCHPAD_LIMIT = 50000


MAX_TOOL_CALLS = 500
MAX_ACCUMULATED_CHUNKS = 10000
MAX_CONVERSATION_HISTORY = 100
MAX_LOG_MESSAGE_LENGTH = 4096
