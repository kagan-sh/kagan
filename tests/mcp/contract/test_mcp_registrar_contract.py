"""Contracts for MCP registrar public surface."""

from __future__ import annotations

import kagan.mcp.registrars as registrars

_EXPECTED_EXPORTS = {
    "ToolRegistrationContext",
    "SharedToolRegistrationContext",
    "register_full_mode_tools",
    "register_shared_tools",
}


def test_registrar_exports_are_single_authoritative_path() -> None:
    """Only explicit shared/full registration APIs are exported."""
    assert set(registrars.__all__) == _EXPECTED_EXPORTS


def test_auto_registrar_api_is_not_exported() -> None:
    assert not hasattr(registrars, "register_auto_tools")
    assert not hasattr(registrars, "discover_exposed_methods")
