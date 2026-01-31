"""Tests for pre-flight check orchestration in __main__.py."""

from __future__ import annotations

from unittest.mock import patch

import pytest

pytestmark = pytest.mark.integration


class TestPreflightAgentSelection:
    """Tests for agent selection during pre-flight checks."""

    def test_uses_available_agent_when_any_available(self):
        """When at least one agent is available, pre-flight should use it.

        This tests the fix for the bug where:
        1. any_agent_available() returned True (OpenCode installed)
        2. But pre-flight still checked Claude (from config) which wasn't installed
        3. Resulting in unnecessary troubleshooting screen
        """
        from kagan.data.builtin_agents import (
            any_agent_available,
            get_first_available_agent,
        )

        # Simulate: OpenCode available, Claude not available
        def mock_which(cmd: str) -> str | None:
            if cmd == "opencode":
                return "/usr/local/bin/opencode"
            return None  # claude, npx not available

        with patch("shutil.which", side_effect=mock_which):
            # Step 1: any_agent_available() should return True
            assert any_agent_available() is True

            # Step 2: get_first_available_agent() should return OpenCode
            agent = get_first_available_agent()
            assert agent is not None
            assert agent.config.short_name == "opencode"
            assert agent.config.name == "OpenCode"

    def test_uses_claude_when_both_available(self):
        """When both agents are available, Claude should be preferred (priority)."""
        from kagan.data.builtin_agents import get_first_available_agent

        def mock_which(cmd: str) -> str | None:
            if cmd in ("claude", "opencode", "npx"):
                return f"/usr/local/bin/{cmd}"
            return None

        with patch("shutil.which", side_effect=mock_which):
            agent = get_first_available_agent()
            assert agent is not None
            assert agent.config.short_name == "claude"

    def test_no_agents_available_returns_none(self):
        """When no agents are available, get_first_available_agent returns None."""
        from kagan.data.builtin_agents import (
            any_agent_available,
            get_first_available_agent,
        )

        with patch("shutil.which", return_value=None):
            assert any_agent_available() is False
            assert get_first_available_agent() is None

    def test_npx_only_does_not_make_claude_available(self):
        """When only npx is installed (no actual agents), Claude should NOT be available.

        This is the key bug fix: on a system with node/nvm installed, npx exists
        but claude-code-acp package is not installed. Just having npx should NOT
        count as Claude being available.
        """
        from kagan.data.builtin_agents import (
            any_agent_available,
            get_first_available_agent,
        )

        def mock_which(cmd: str) -> str | None:
            # Simulate: npx and tmux available, but NO agent binaries
            if cmd in ("npx", "tmux"):
                return f"/usr/bin/{cmd}"
            return None  # claude, opencode, claude-code-acp not available

        with patch("shutil.which", side_effect=mock_which):
            # No agents should be available
            assert any_agent_available() is False
            assert get_first_available_agent() is None

    def test_opencode_available_with_npx_but_no_claude(self):
        """When OpenCode is installed but Claude is not (even with npx), use OpenCode.

        This is the Ubuntu scenario: node/npm/npx installed, OpenCode installed,
        but Claude Code not installed.
        """
        from kagan.data.builtin_agents import (
            any_agent_available,
            get_first_available_agent,
        )

        def mock_which(cmd: str) -> str | None:
            # Simulate: npx, tmux, and opencode available, but NOT claude or claude-code-acp
            if cmd in ("npx", "tmux", "opencode"):
                return f"/usr/bin/{cmd}"
            return None

        with patch("shutil.which", side_effect=mock_which):
            assert any_agent_available() is True
            agent = get_first_available_agent()
            assert agent is not None
            # Should be OpenCode, NOT Claude (even though npx exists)
            assert agent.config.short_name == "opencode"


class TestPreflightDetectIssues:
    """Tests for detect_issues with available agent."""

    async def test_detect_issues_passes_with_available_agent(self):
        """When agent is available, detect_issues should not find agent issues."""
        from kagan.data.builtin_agents import get_first_available_agent
        from kagan.ui.screens.troubleshooting import detect_issues

        def mock_which(cmd: str) -> str | None:
            # Simulate OpenCode available, tmux available, git available
            if cmd in ("opencode", "tmux", "git"):
                return f"/usr/local/bin/{cmd}"
            return None

        with (
            patch("shutil.which", side_effect=mock_which),
            patch(
                "kagan.ui.screens.troubleshooting._check_git_version",
                return_value=None,
            ),
            patch(
                "kagan.ui.screens.troubleshooting._check_git_user",
                return_value=None,
            ),
        ):
            agent = get_first_available_agent()
            assert agent is not None

            # Run detect_issues with the available agent
            result = await detect_issues(
                check_lock=False,
                agent_config=agent.config,
                agent_name=agent.config.name,
                agent_install_command=agent.install_command,
            )

            # Should not have blocking issues (agent is available)
            assert result.has_blocking_issues is False

    async def test_detect_issues_fails_with_unavailable_agent(self):
        """When agent is not available, detect_issues should report it."""
        from kagan.data.builtin_agents import BUILTIN_AGENTS
        from kagan.ui.screens.troubleshooting import IssueType, detect_issues

        # Get Claude agent config but simulate it's not installed
        claude = BUILTIN_AGENTS["claude"]

        def mock_which(cmd: str) -> str | None:
            # Only tmux and git available, no agents
            if cmd in ("tmux", "git"):
                return f"/usr/bin/{cmd}"
            return None

        with (
            patch("shutil.which", side_effect=mock_which),
            patch(
                "kagan.ui.screens.troubleshooting._check_git_version",
                return_value=None,
            ),
            patch(
                "kagan.ui.screens.troubleshooting._check_git_user",
                return_value=None,
            ),
        ):
            result = await detect_issues(
                check_lock=False,
                agent_config=claude.config,
                agent_name=claude.config.name,
                agent_install_command=claude.install_command,
            )

            # Should have blocking issue for missing agent
            assert result.has_blocking_issues is True
            issue_types = [issue.preset.type for issue in result.issues]
            assert (
                IssueType.AGENT_MISSING in issue_types or IssueType.ACP_AGENT_MISSING in issue_types
            )


class TestPreflightFlowIntegration:
    """Integration tests for the complete pre-flight flow logic."""

    async def test_preflight_flow_skips_troubleshooting_with_available_agent(self):
        """Complete flow: available agent should skip troubleshooting.

        This is the key integration test that would have caught the original bug.
        It tests the actual orchestration logic from __main__.py.
        """
        from kagan.data.builtin_agents import any_agent_available, get_first_available_agent
        from kagan.ui.screens.troubleshooting import detect_issues

        def mock_which(cmd: str) -> str | None:
            # Simulate: Only OpenCode available (not Claude), plus git
            if cmd == "opencode":
                return "/usr/local/bin/opencode"
            if cmd in ("tmux", "git"):
                return f"/usr/bin/{cmd}"
            return None  # claude, npx not available

        with (
            patch("shutil.which", side_effect=mock_which),
            patch(
                "kagan.ui.screens.troubleshooting._check_git_version",
                return_value=None,
            ),
            patch(
                "kagan.ui.screens.troubleshooting._check_git_user",
                return_value=None,
            ),
        ):
            # This is the logic from __main__.py

            # Step 1: Check if ANY agent is available
            if not any_agent_available():
                pytest.fail("Should have at least one agent available")

            # Step 2: Get the first available agent (this is the fix!)
            best_agent = get_first_available_agent()
            assert best_agent is not None, "Should have found OpenCode"
            assert best_agent.config.short_name == "opencode"

            # Step 3: Run detect_issues with the AVAILABLE agent
            result = await detect_issues(
                check_lock=False,
                agent_config=best_agent.config,
                agent_name=best_agent.config.name,
                agent_install_command=best_agent.install_command,
            )

            # Step 4: Should NOT have blocking issues
            assert result.has_blocking_issues is False, (
                f"Should not show troubleshooting when agent is available. "
                f"Issues found: {[i.preset.type for i in result.issues]}"
            )

    def test_preflight_flow_shows_troubleshooting_when_no_agents(self):
        """When no agents are available, should show troubleshooting."""
        from kagan.data.builtin_agents import any_agent_available
        from kagan.ui.screens.troubleshooting import create_no_agents_issues

        with patch("shutil.which", return_value=None):
            # Step 1: No agents available
            assert any_agent_available() is False

            # Step 2: Create issues for all agents
            issues = create_no_agents_issues()
            assert len(issues) >= 2  # At least Claude and OpenCode

            # Verify each issue has install instructions
            for issue in issues:
                assert issue.preset.hint is not None
                assert len(issue.preset.hint) > 0
