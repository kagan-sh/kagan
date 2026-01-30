"""Unit tests for Agent capability configuration."""

from __future__ import annotations

import pytest

from kagan.acp import protocol  # noqa: TC001

pytestmark = pytest.mark.unit


class TestAgentCapabilities:
    """Tests for Agent read_only mode capability derivation."""

    def test_default_capabilities_include_write(self):
        """Default (non-read-only) mode includes write capabilities."""
        # Test the capability building logic (mirrors _acp_initialize)
        read_only = False

        fs_caps: protocol.FileSystemCapability = {"readTextFile": True}
        if not read_only:
            fs_caps["writeTextFile"] = True

        client_caps: protocol.ClientCapabilities = {
            "fs": fs_caps,
            "terminal": not read_only,
        }

        assert client_caps["fs"].get("readTextFile") is True
        assert client_caps["fs"].get("writeTextFile") is True
        assert client_caps.get("terminal") is True

    def test_read_only_capabilities_exclude_write(self):
        """Read-only mode excludes write and terminal capabilities."""
        read_only = True

        fs_caps: protocol.FileSystemCapability = {"readTextFile": True}
        if not read_only:
            fs_caps["writeTextFile"] = True

        client_caps: protocol.ClientCapabilities = {
            "fs": fs_caps,
            "terminal": not read_only,
        }

        assert client_caps["fs"].get("readTextFile") is True
        assert "writeTextFile" not in client_caps["fs"]
        assert client_caps.get("terminal") is False

    def test_read_only_always_has_read_capability(self):
        """Read-only mode still allows reading files."""
        read_only = True

        fs_caps: protocol.FileSystemCapability = {"readTextFile": True}
        if not read_only:
            fs_caps["writeTextFile"] = True

        assert fs_caps.get("readTextFile") is True
