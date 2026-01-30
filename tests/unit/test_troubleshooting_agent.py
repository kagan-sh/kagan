"""Unit tests for troubleshooting screen detect_issues() - agent detection and presets."""

from __future__ import annotations

import pytest

from kagan.config import AgentConfig
from kagan.ui.screens.troubleshooting import (
    ISSUE_PRESETS,
    DetectedIssue,
    IssueSeverity,
    IssueType,
    PreflightResult,
    detect_issues,
)

pytestmark = pytest.mark.unit


class TestDetectIssuesAgent:
    """Test agent availability detection."""

    def test_detects_missing_agent(self, mocker):
        """Missing agent is detected when agent_config is provided."""
        agent_config = AgentConfig(
            identity="test.ai",
            name="Test Agent",
            short_name="test",
            run_command={"*": "test-acp"},
            interactive_command={"*": "test-cli"},
        )

        def mock_which(cmd):
            if cmd == "tmux":
                return "/usr/bin/tmux"
            return None  # Agent not found

        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which)

        result = detect_issues(
            agent_config=agent_config,
            agent_name="Test Agent",
            agent_install_command="pip install test-agent",
        )

        assert result.has_blocking_issues
        agent_issues = [i for i in result.issues if i.preset.type == IssueType.AGENT_MISSING]
        assert len(agent_issues) == 1
        assert "Test Agent" in agent_issues[0].preset.message
        assert "pip install test-agent" in agent_issues[0].preset.hint

    def test_agent_available_no_issue(self, mocker):
        """Available agent produces no issue."""
        agent_config = AgentConfig(
            identity="test.ai",
            name="Test Agent",
            short_name="test",
            run_command={"*": "test-acp"},
            interactive_command={"*": "test-cli"},
        )

        def mock_which(cmd):
            return f"/usr/bin/{cmd}"  # All commands found

        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which)

        result = detect_issues(
            agent_config=agent_config,
            agent_name="Test Agent",
        )

        assert not any(i.preset.type == IssueType.AGENT_MISSING for i in result.issues)

    def test_no_agent_config_skips_check(self, mocker):
        """No agent_config skips agent check."""
        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux")

        result = detect_issues(agent_config=None)

        assert not any(i.preset.type == IssueType.AGENT_MISSING for i in result.issues)


class TestDetectIssuesMultiple:
    """Test multiple issues detected together."""

    def test_multiple_issues_detected(self, mocker):
        """Multiple issues can be detected and returned together."""
        agent_config = AgentConfig(
            identity="test.ai",
            name="Test Agent",
            short_name="test",
            run_command={"*": "test-acp"},
            interactive_command={"*": "test-cli"},
        )

        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value=None)

        result = detect_issues(
            check_lock=True,
            lock_acquired=False,
            agent_config=agent_config,
            agent_name="Test Agent",
        )

        # Should have lock, tmux, agent (interactive), and ACP agent (run) issues
        issue_types = {i.preset.type for i in result.issues}
        assert IssueType.INSTANCE_LOCKED in issue_types
        assert IssueType.TMUX_MISSING in issue_types
        assert IssueType.AGENT_MISSING in issue_types
        assert IssueType.ACP_AGENT_MISSING in issue_types
        assert len(result.issues) == 4


class TestPreflightResult:
    """Test PreflightResult dataclass."""

    def test_has_blocking_issues_true(self):
        """has_blocking_issues returns True when blocking issues exist."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        result = PreflightResult(issues=issues)
        assert result.has_blocking_issues

    def test_has_blocking_issues_false_empty(self):
        """has_blocking_issues returns False when no issues."""
        result = PreflightResult(issues=[])
        assert not result.has_blocking_issues


class TestIssuePresets:
    """Test that all issue presets are properly defined."""

    def test_all_issue_types_have_presets(self):
        """Every IssueType has a corresponding preset."""
        for issue_type in IssueType:
            assert issue_type in ISSUE_PRESETS, f"Missing preset for {issue_type}"

    def test_all_presets_have_required_fields(self):
        """All presets have required fields populated."""
        for issue_type, preset in ISSUE_PRESETS.items():
            assert preset.type == issue_type
            assert preset.severity in IssueSeverity
            assert preset.icon
            assert preset.title
            assert preset.message
            assert preset.hint

    def test_all_presets_are_blocking(self):
        """All current presets are blocking severity (per plan)."""
        for preset in ISSUE_PRESETS.values():
            assert preset.severity == IssueSeverity.BLOCKING
