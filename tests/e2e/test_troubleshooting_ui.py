"""UI tests for TroubleshootingApp rendering."""

from __future__ import annotations

import pytest

from kagan.ui.screens.troubleshooting import (
    ISSUE_PRESETS,
    DetectedIssue,
    IssuePreset,
    IssueSeverity,
    IssueType,
    TroubleshootingApp,
)

pytestmark = pytest.mark.e2e


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

    @pytest.mark.parametrize(
        "issue_type,has_url",
        [
            (IssueType.WINDOWS_OS, True),
            (IssueType.INSTANCE_LOCKED, False),
            (IssueType.TMUX_MISSING, False),
        ],
        ids=["windows_with_url", "lock_no_url", "tmux_no_url"],
    )
    async def test_displays_issue_type(self, issue_type: IssueType, has_url: bool):
        """App displays issue type correctly with expected URL presence."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[issue_type])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            # Check issue card exists
            issue_cards = list(app.query(".issue-card"))
            assert len(issue_cards) == 1

            # Check title exists
            titles = list(app.query(".issue-card-title"))
            assert len(titles) >= 1

            # Check hint exists
            hints = list(app.query(".issue-card-hint"))
            assert len(hints) >= 1

            # Verify URL presence based on issue type
            urls = list(app.query(".issue-card-url"))
            if has_url:
                assert len(urls) >= 1
            else:
                assert len(urls) == 0

    async def test_displays_agent_issue(self):
        """App displays agent missing issue correctly."""
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

    @pytest.mark.parametrize("key", ["q", "escape", "enter"])
    async def test_quit_keys(self, key: str):
        """Pressing quit keys exits the app."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()
            await pilot.press(key)
            await pilot.pause()
            assert not app.is_running

    @pytest.mark.parametrize(
        "element_id",
        ["#troubleshoot-exit-hint", "#troubleshoot-resolve-hint"],
        ids=["exit_hint", "resolve_hint"],
    )
    async def test_displays_hint_elements(self, element_id: str):
        """App displays hint elements."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            hint = app.query_one(element_id)
            assert hint is not None

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

    @pytest.mark.parametrize(
        "container_id",
        ["#troubleshoot-container", "#troubleshoot-card", "#troubleshoot-issues"],
        ids=["main_container", "card", "issues"],
    )
    async def test_container_structure(self, container_id: str):
        """App has correct container structure."""
        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        app = TroubleshootingApp(issues)

        async with app.run_test(size=(100, 30)) as pilot:
            await pilot.pause()

            assert app.query_one(container_id)
