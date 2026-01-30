"""Tests for ACP command resolution with npx fallback."""

from __future__ import annotations

import pytest

from kagan.ui.screens.troubleshooting import IssueType, resolve_acp_command

pytestmark = pytest.mark.unit


class TestResolveACPCommand:
    """Test ACP command resolution with npx fallback."""

    def test_npx_command_uses_global_binary_when_available(self, mocker):
        """When the binary is globally installed, use it directly instead of npx."""

        def mock_which(cmd):
            if cmd == "claude-code-acp":
                return "/usr/local/bin/claude-code-acp"
            return None

        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which)

        result = resolve_acp_command("npx claude-code-acp", "Claude Code")

        assert result.resolved_command == "claude-code-acp"
        assert result.issue is None
        assert result.used_fallback is True

    def test_npx_command_uses_npx_when_no_global_binary(self, mocker):
        """When binary not installed globally but npx is available, use npx."""

        def mock_which(cmd):
            if cmd == "npx":
                return "/usr/local/bin/npx"
            return None

        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which)

        result = resolve_acp_command("npx claude-code-acp", "Claude Code")

        assert result.resolved_command == "npx claude-code-acp"
        assert result.issue is None
        assert result.used_fallback is False

    def test_npx_command_error_when_neither_available(self, mocker):
        """When neither npx nor global binary available, return error."""
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value=None)

        result = resolve_acp_command("npx claude-code-acp", "Claude Code")

        assert result.resolved_command is None
        assert result.issue is not None
        assert result.issue.preset.type == IssueType.NPX_MISSING
        assert "npx" in result.issue.preset.message.lower()
        assert "claude-code-acp" in result.issue.preset.hint

    def test_npx_scoped_package_extracts_binary_name(self, mocker):
        """Scoped packages like @anthropic-ai/claude-code-acp extract binary correctly."""

        def mock_which(cmd):
            if cmd == "claude-code-acp":
                return "/usr/local/bin/claude-code-acp"
            return None

        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which)

        result = resolve_acp_command("npx @anthropic-ai/claude-code-acp", "Claude Code")

        assert result.resolved_command == "claude-code-acp"
        assert result.used_fallback is True

    def test_non_npx_command_found(self, mocker):
        """Non-npx command that is found in PATH works normally."""

        def mock_which(cmd):
            if cmd == "opencode":
                return "/usr/local/bin/opencode"
            return None

        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which)

        result = resolve_acp_command("opencode acp", "OpenCode")

        assert result.resolved_command == "opencode acp"
        assert result.issue is None
        assert result.used_fallback is False

    def test_non_npx_command_not_found(self, mocker):
        """Non-npx command that is not in PATH returns error."""
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value=None)

        result = resolve_acp_command("opencode acp", "OpenCode")

        assert result.resolved_command is None
        assert result.issue is not None
        assert result.issue.preset.type == IssueType.ACP_AGENT_MISSING

    def test_npx_command_preserves_extra_args(self, mocker):
        """Extra args in npx command are preserved when falling back to global binary."""

        def mock_which(cmd):
            if cmd == "claude-code-acp":
                return "/usr/local/bin/claude-code-acp"
            return None

        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which)

        result = resolve_acp_command("npx claude-code-acp --debug", "Claude Code")

        assert result.resolved_command == "claude-code-acp --debug"
