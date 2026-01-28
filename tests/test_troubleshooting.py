"""Tests for the troubleshooting screen.

Tests cover all pre-flight check variations:
- Windows OS detection
- Instance lock detection
- tmux missing detection
- Agent missing detection
- Multiple issues displayed together
"""

from __future__ import annotations

from unittest.mock import patch

from kagan.config import AgentConfig
from kagan.ui.screens.troubleshooting import (
    ISSUE_PRESETS,
    DetectedIssue,
    IssueSeverity,
    IssueType,
    PreflightResult,
    TroubleshootingApp,
    detect_issues,
    resolve_acp_command,
)

# =============================================================================
# Unit Tests for detect_issues()
# =============================================================================


class TestDetectIssuesWindows:
    """Test Windows OS detection."""

    def test_detects_windows_os(self):
        """Windows detection returns a blocking issue."""
        with patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Windows"):
            result = detect_issues()

        assert result.has_blocking_issues
        assert len(result.issues) == 1
        assert result.issues[0].preset.type == IssueType.WINDOWS_OS
        assert result.issues[0].preset.severity == IssueSeverity.BLOCKING

    def test_windows_detection_exits_early(self):
        """Windows detection should return only Windows issue, skipping other checks."""
        with (
            patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Windows"),
            patch("kagan.ui.screens.troubleshooting.shutil.which", return_value=None),
        ):
            result = detect_issues(check_lock=True, lock_acquired=False)

        # Should only have Windows issue, not lock or tmux issues
        assert len(result.issues) == 1
        assert result.issues[0].preset.type == IssueType.WINDOWS_OS

    def test_non_windows_passes(self):
        """Non-Windows platforms pass the Windows check."""
        with (
            patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin"),
            patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux"),
        ):
            result = detect_issues()

        # No Windows issue
        assert not any(i.preset.type == IssueType.WINDOWS_OS for i in result.issues)


class TestDetectIssuesInstanceLock:
    """Test instance lock detection."""

    def test_detects_lock_failure(self):
        """Lock failure is detected when check_lock=True and lock_acquired=False."""
        with (
            patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin"),
            patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux"),
        ):
            result = detect_issues(check_lock=True, lock_acquired=False)

        assert result.has_blocking_issues
        assert any(i.preset.type == IssueType.INSTANCE_LOCKED for i in result.issues)

    def test_lock_success_no_issue(self):
        """Successful lock acquisition produces no issue."""
        with (
            patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin"),
            patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux"),
        ):
            result = detect_issues(check_lock=True, lock_acquired=True)

        assert not any(i.preset.type == IssueType.INSTANCE_LOCKED for i in result.issues)

    def test_lock_not_checked_by_default(self):
        """Lock is not checked when check_lock=False (default)."""
        with (
            patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin"),
            patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux"),
        ):
            # Even with lock_acquired=False, should not report issue if check_lock=False
            result = detect_issues(check_lock=False, lock_acquired=False)

        assert not any(i.preset.type == IssueType.INSTANCE_LOCKED for i in result.issues)


class TestDetectIssuesTmux:
    """Test tmux availability detection."""

    def test_detects_missing_tmux(self):
        """Missing tmux is detected."""
        with (
            patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin"),
            patch("kagan.ui.screens.troubleshooting.shutil.which", return_value=None),
        ):
            result = detect_issues()

        assert result.has_blocking_issues
        assert any(i.preset.type == IssueType.TMUX_MISSING for i in result.issues)

    def test_tmux_available_no_issue(self):
        """Available tmux produces no issue."""
        with (
            patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin"),
            patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux"),
        ):
            result = detect_issues()

        assert not any(i.preset.type == IssueType.TMUX_MISSING for i in result.issues)


class TestDetectIssuesAgent:
    """Test agent availability detection."""

    def test_detects_missing_agent(self):
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

        with (
            patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin"),
            patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which),
        ):
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

    def test_agent_available_no_issue(self):
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

        with (
            patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin"),
            patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which),
        ):
            result = detect_issues(
                agent_config=agent_config,
                agent_name="Test Agent",
            )

        assert not any(i.preset.type == IssueType.AGENT_MISSING for i in result.issues)

    def test_no_agent_config_skips_check(self):
        """No agent_config skips agent check."""
        with (
            patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin"),
            patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux"),
        ):
            result = detect_issues(agent_config=None)

        assert not any(i.preset.type == IssueType.AGENT_MISSING for i in result.issues)


class TestResolveACPCommand:
    """Test ACP command resolution with npx fallback."""

    def test_npx_command_uses_global_binary_when_available(self):
        """When the binary is globally installed, use it directly instead of npx."""

        def mock_which(cmd):
            if cmd == "claude-code-acp":
                return "/usr/local/bin/claude-code-acp"
            return None

        with patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which):
            result = resolve_acp_command("npx claude-code-acp", "Claude Code")

        assert result.resolved_command == "claude-code-acp"
        assert result.issue is None
        assert result.used_fallback is True

    def test_npx_command_uses_npx_when_no_global_binary(self):
        """When binary not installed globally but npx is available, use npx."""

        def mock_which(cmd):
            if cmd == "npx":
                return "/usr/local/bin/npx"
            return None

        with patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which):
            result = resolve_acp_command("npx claude-code-acp", "Claude Code")

        assert result.resolved_command == "npx claude-code-acp"
        assert result.issue is None
        assert result.used_fallback is False

    def test_npx_command_error_when_neither_available(self):
        """When neither npx nor global binary available, return error."""
        with patch("kagan.ui.screens.troubleshooting.shutil.which", return_value=None):
            result = resolve_acp_command("npx claude-code-acp", "Claude Code")

        assert result.resolved_command is None
        assert result.issue is not None
        assert result.issue.preset.type == IssueType.NPX_MISSING
        assert "npx" in result.issue.preset.message.lower()
        assert "claude-code-acp" in result.issue.preset.hint

    def test_npx_scoped_package_extracts_binary_name(self):
        """Scoped packages like @anthropic-ai/claude-code-acp extract binary correctly."""

        def mock_which(cmd):
            if cmd == "claude-code-acp":
                return "/usr/local/bin/claude-code-acp"
            return None

        with patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which):
            result = resolve_acp_command("npx @anthropic-ai/claude-code-acp", "Claude Code")

        assert result.resolved_command == "claude-code-acp"
        assert result.used_fallback is True

    def test_non_npx_command_found(self):
        """Non-npx command that is found in PATH works normally."""

        def mock_which(cmd):
            if cmd == "opencode":
                return "/usr/local/bin/opencode"
            return None

        with patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which):
            result = resolve_acp_command("opencode acp", "OpenCode")

        assert result.resolved_command == "opencode acp"
        assert result.issue is None
        assert result.used_fallback is False

    def test_non_npx_command_not_found(self):
        """Non-npx command that is not in PATH returns error."""
        with patch("kagan.ui.screens.troubleshooting.shutil.which", return_value=None):
            result = resolve_acp_command("opencode acp", "OpenCode")

        assert result.resolved_command is None
        assert result.issue is not None
        assert result.issue.preset.type == IssueType.ACP_AGENT_MISSING

    def test_npx_command_preserves_extra_args(self):
        """Extra args in npx command are preserved when falling back to global binary."""

        def mock_which(cmd):
            if cmd == "claude-code-acp":
                return "/usr/local/bin/claude-code-acp"
            return None

        with patch("kagan.ui.screens.troubleshooting.shutil.which", side_effect=mock_which):
            result = resolve_acp_command("npx claude-code-acp --debug", "Claude Code")

        assert result.resolved_command == "claude-code-acp --debug"


class TestDetectIssuesMultiple:
    """Test multiple issues detected together."""

    def test_multiple_issues_detected(self):
        """Multiple issues can be detected and returned together."""
        agent_config = AgentConfig(
            identity="test.ai",
            name="Test Agent",
            short_name="test",
            run_command={"*": "test-acp"},
            interactive_command={"*": "test-cli"},
        )

        with (
            patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin"),
            patch("kagan.ui.screens.troubleshooting.shutil.which", return_value=None),
        ):
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


# =============================================================================
# UI Tests for TroubleshootingApp
# =============================================================================


class TestTroubleshootingAppUI:
    """Test TroubleshootingApp UI rendering."""

    async def test_displays_single_issue(self):
        """App displays a single issue correctly."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            # Check key elements exist
            assert app.query_one("#troubleshoot-title")
            assert app.query_one("#troubleshoot-count")

            # Check issue card exists
            issue_cards = list(app.query(".issue-card"))
            assert len(issue_cards) == 1

    async def test_displays_multiple_issues(self):
        """App displays multiple issues correctly."""
        issues = [
            DetectedIssue(preset=ISSUE_PRESETS[IssueType.INSTANCE_LOCKED]),
            DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING]),
        ]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            # Check both issue cards exist
            issue_cards = list(app.query(".issue-card"))
            assert len(issue_cards) == 2

    async def test_displays_windows_issue(self):
        """App displays Windows OS issue correctly."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.WINDOWS_OS])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            # Check issue card exists with URL (Windows has a URL)
            issue_cards = list(app.query(".issue-card"))
            assert len(issue_cards) == 1

            # Windows issue should have a URL
            urls = list(app.query(".issue-card-url"))
            assert len(urls) >= 1

    async def test_displays_lock_issue(self):
        """App displays instance locked issue correctly."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.INSTANCE_LOCKED])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            # Check structure
            issue_cards = list(app.query(".issue-card"))
            assert len(issue_cards) == 1

            titles = list(app.query(".issue-card-title"))
            assert len(titles) >= 1

    async def test_displays_tmux_issue(self):
        """App displays tmux missing issue correctly."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            issue_cards = list(app.query(".issue-card"))
            assert len(issue_cards) == 1

            # Check hint exists
            hints = list(app.query(".issue-card-hint"))
            assert len(hints) >= 1

    async def test_displays_agent_issue(self):
        """App displays agent missing issue correctly."""
        from kagan.ui.screens.troubleshooting import IssuePreset

        custom_preset = IssuePreset(
            type=IssueType.AGENT_MISSING,
            severity=IssueSeverity.BLOCKING,
            icon="[!]",
            title="Default Agent Not Installed",
            message="The default agent (Claude Code) was not found in PATH.",
            hint="Install: curl -fsSL https://claude.ai/install.sh | bash",
        )
        issues = [DetectedIssue(preset=custom_preset, details="Claude Code")]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            issue_cards = list(app.query(".issue-card"))
            assert len(issue_cards) == 1

            titles = list(app.query(".issue-card-title"))
            assert len(titles) >= 1

    async def test_displays_kagan_logo(self):
        """App displays the KAGAN logo."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            logo = app.query_one("#troubleshoot-logo")
            assert logo is not None

    async def test_q_quits_app(self):
        """Pressing 'q' quits the app."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            await pilot.press("q")
            await pilot.pause()
            # App should have exited
            assert not app.is_running

    async def test_escape_quits_app(self):
        """Pressing 'escape' quits the app."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            await pilot.press("escape")
            await pilot.pause()
            assert not app.is_running

    async def test_enter_quits_app(self):
        """Pressing 'enter' quits the app."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            await pilot.press("enter")
            await pilot.pause()
            assert not app.is_running

    async def test_displays_exit_hint(self):
        """App displays exit hint."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            exit_hint = app.query_one("#troubleshoot-exit-hint")
            assert exit_hint is not None

    async def test_displays_resolve_hint(self):
        """App displays resolve hint."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            resolve_hint = app.query_one("#troubleshoot-resolve-hint")
            assert resolve_hint is not None

    async def test_displays_url_when_present(self):
        """App displays URL link when preset has one."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.WINDOWS_OS])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            urls = list(app.query(".issue-card-url"))
            assert len(urls) >= 1

    async def test_no_url_when_absent(self):
        """App does not display URL when preset doesn't have one."""
        # INSTANCE_LOCKED has no URL
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.INSTANCE_LOCKED])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            urls = list(app.query(".issue-card-url"))
            # Should have no URLs for this issue type
            assert len(urls) == 0

    async def test_all_issue_cards_have_required_elements(self):
        """Every issue card has title, message, and hint."""
        issues = [
            DetectedIssue(preset=ISSUE_PRESETS[IssueType.INSTANCE_LOCKED]),
            DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING]),
        ]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            # Each issue should have title, message, hint
            titles = list(app.query(".issue-card-title"))
            messages = list(app.query(".issue-card-message"))
            hints = list(app.query(".issue-card-hint"))

            assert len(titles) == 2
            assert len(messages) == 2
            assert len(hints) == 2

    async def test_container_structure(self):
        """App has correct container structure."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            # Check main container exists
            assert app.query_one("#troubleshoot-container")
            assert app.query_one("#troubleshoot-card")
            assert app.query_one("#troubleshoot-issues")
