"""Unit tests for troubleshooting screen detect_issues() function - OS and lock detection."""

from __future__ import annotations

import pytest

from kagan.ui.screens.troubleshooting import (
    IssueSeverity,
    IssueType,
    detect_issues,
)

pytestmark = pytest.mark.unit


class TestDetectIssuesWindows:
    """Test Windows OS detection."""

    async def test_detects_windows_os(self, mocker):
        """Windows detection returns a blocking issue."""
        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Windows")

        result = await detect_issues()

        assert result.has_blocking_issues
        assert len(result.issues) == 1
        assert result.issues[0].preset.type == IssueType.WINDOWS_OS
        assert result.issues[0].preset.severity == IssueSeverity.BLOCKING

    async def test_windows_detection_exits_early(self, mocker):
        """Windows detection should return only Windows issue, skipping other checks."""
        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Windows")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value=None)

        result = await detect_issues(check_lock=True, lock_acquired=False)

        # Should only have Windows issue, not lock or tmux issues
        assert len(result.issues) == 1
        assert result.issues[0].preset.type == IssueType.WINDOWS_OS

    async def test_non_windows_passes(self, mocker):
        """Non-Windows platforms pass the Windows check."""
        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux")
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_version", return_value=None)
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_user", return_value=None)
        mocker.patch(
            "kagan.ui.screens.troubleshooting._check_terminal_truecolor", return_value=None
        )

        result = await detect_issues()

        # No Windows issue
        assert not any(i.preset.type == IssueType.WINDOWS_OS for i in result.issues)


class TestDetectIssuesInstanceLock:
    """Test instance lock detection."""

    async def test_detects_lock_failure(self, mocker):
        """Lock failure is detected when check_lock=True and lock_acquired=False."""
        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux")
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_version", return_value=None)
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_user", return_value=None)
        mocker.patch(
            "kagan.ui.screens.troubleshooting._check_terminal_truecolor", return_value=None
        )

        result = await detect_issues(check_lock=True, lock_acquired=False)

        assert result.has_blocking_issues
        assert any(i.preset.type == IssueType.INSTANCE_LOCKED for i in result.issues)

    async def test_lock_success_no_issue(self, mocker):
        """Successful lock acquisition produces no issue."""
        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux")
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_version", return_value=None)
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_user", return_value=None)
        mocker.patch(
            "kagan.ui.screens.troubleshooting._check_terminal_truecolor", return_value=None
        )

        result = await detect_issues(check_lock=True, lock_acquired=True)

        assert not any(i.preset.type == IssueType.INSTANCE_LOCKED for i in result.issues)

    async def test_lock_not_checked_by_default(self, mocker):
        """Lock is not checked when check_lock=False (default)."""
        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux")
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_version", return_value=None)
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_user", return_value=None)
        mocker.patch(
            "kagan.ui.screens.troubleshooting._check_terminal_truecolor", return_value=None
        )

        # Even with lock_acquired=False, should not report issue if check_lock=False
        result = await detect_issues(check_lock=False, lock_acquired=False)

        assert not any(i.preset.type == IssueType.INSTANCE_LOCKED for i in result.issues)


class TestDetectIssuesTmux:
    """Test tmux availability detection."""

    async def test_detects_missing_tmux(self, mocker):
        """Missing tmux is detected as a warning (not blocking)."""
        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value=None)
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_version", return_value=None)
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_user", return_value=None)
        mocker.patch(
            "kagan.ui.screens.troubleshooting._check_terminal_truecolor", return_value=None
        )

        result = await detect_issues()

        # tmux missing is now a warning, not blocking
        assert not result.has_blocking_issues
        tmux_issues = [i for i in result.issues if i.preset.type == IssueType.TMUX_MISSING]
        assert len(tmux_issues) == 1
        assert tmux_issues[0].preset.severity == IssueSeverity.WARNING

    async def test_tmux_available_no_issue(self, mocker):
        """Available tmux produces no issue."""
        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux")
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_version", return_value=None)
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_user", return_value=None)
        mocker.patch(
            "kagan.ui.screens.troubleshooting._check_terminal_truecolor", return_value=None
        )

        result = await detect_issues()

        assert not any(i.preset.type == IssueType.TMUX_MISSING for i in result.issues)


class TestDetectIssuesTerminal:
    """Test terminal truecolor detection."""

    async def test_detects_no_truecolor_support(self, mocker):
        """Missing truecolor support is detected as a warning."""
        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux")
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_version", return_value=None)
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_user", return_value=None)
        mocker.patch("kagan.terminal.supports_truecolor", return_value=False)
        mocker.patch("kagan.terminal.get_terminal_name", return_value="macOS Terminal.app")

        result = await detect_issues()

        # Terminal warning is not blocking
        assert not result.has_blocking_issues
        terminal_issues = [
            i for i in result.issues if i.preset.type == IssueType.TERMINAL_NO_TRUECOLOR
        ]
        assert len(terminal_issues) == 1
        assert terminal_issues[0].preset.severity == IssueSeverity.WARNING
        assert "macOS Terminal.app" in terminal_issues[0].preset.message

    async def test_truecolor_supported_no_issue(self, mocker):
        """Truecolor support produces no issue."""
        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux")
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_version", return_value=None)
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_user", return_value=None)
        mocker.patch("kagan.terminal.supports_truecolor", return_value=True)

        result = await detect_issues()

        assert not any(i.preset.type == IssueType.TERMINAL_NO_TRUECOLOR for i in result.issues)

    async def test_terminal_check_can_be_disabled(self, mocker):
        """Terminal check can be disabled via check_terminal=False."""
        mocker.patch("kagan.ui.screens.troubleshooting.platform.system", return_value="Darwin")
        mocker.patch("kagan.ui.screens.troubleshooting.shutil.which", return_value="/usr/bin/tmux")
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_version", return_value=None)
        mocker.patch("kagan.ui.screens.troubleshooting._check_git_user", return_value=None)
        mocker.patch("kagan.terminal.supports_truecolor", return_value=False)

        result = await detect_issues(check_terminal=False)

        # No terminal issue when check is disabled
        assert not any(i.preset.type == IssueType.TERMINAL_NO_TRUECOLOR for i in result.issues)
