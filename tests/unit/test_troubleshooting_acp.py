"""Tests for ACP command resolution with npx fallback - parametrized."""

from __future__ import annotations

import pytest

from kagan.ui.screens.troubleshooting import IssueType, resolve_acp_command

pytestmark = pytest.mark.unit


class TestResolveACPCommand:
    """Test ACP command resolution with npx fallback."""

    @pytest.mark.parametrize(
        "command,agent_name,which_results,expected_cmd,expected_issue_type,expected_fallback",
        [
            # Global binary available - use it directly
            (
                "npx claude-code-acp",
                "Claude Code",
                {"claude-code-acp": "/usr/local/bin/claude-code-acp"},
                "claude-code-acp",
                None,
                True,
            ),
            # No global binary but npx available - use npx
            (
                "npx claude-code-acp",
                "Claude Code",
                {"npx": "/usr/local/bin/npx"},
                "npx claude-code-acp",
                None,
                False,
            ),
            # Neither npx nor global binary available - error
            (
                "npx claude-code-acp",
                "Claude Code",
                {},
                None,
                IssueType.NPX_MISSING,
                False,
            ),
            # Scoped package with global binary
            (
                "npx @anthropic-ai/claude-code-acp",
                "Claude Code",
                {"claude-code-acp": "/usr/local/bin/claude-code-acp"},
                "claude-code-acp",
                None,
                True,
            ),
            # Non-npx command found
            (
                "opencode acp",
                "OpenCode",
                {"opencode": "/usr/local/bin/opencode"},
                "opencode acp",
                None,
                False,
            ),
            # Non-npx command not found
            (
                "opencode acp",
                "OpenCode",
                {},
                None,
                IssueType.ACP_AGENT_MISSING,
                False,
            ),
            # npx command with extra args preserves args
            (
                "npx claude-code-acp --debug",
                "Claude Code",
                {"claude-code-acp": "/usr/local/bin/claude-code-acp"},
                "claude-code-acp --debug",
                None,
                True,
            ),
        ],
        ids=[
            "global_binary_available",
            "npx_fallback",
            "neither_available",
            "scoped_package",
            "non_npx_found",
            "non_npx_not_found",
            "preserves_extra_args",
        ],
    )
    def test_resolve_acp_command(
        self,
        mocker,
        command: str,
        agent_name: str,
        which_results: dict[str, str],
        expected_cmd: str | None,
        expected_issue_type: IssueType | None,
        expected_fallback: bool,
    ):
        """Test command resolution with various scenarios."""

        def mock_which(cmd):
            return which_results.get(cmd)

        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which)

        result = resolve_acp_command(command, agent_name)

        assert result.resolved_command == expected_cmd
        assert result.used_fallback is expected_fallback

        if expected_issue_type is None:
            assert result.issue is None
        else:
            assert result.issue is not None
            assert result.issue.preset.type == expected_issue_type
