"""Unit tests for the Install Modal in troubleshooting screen."""

from __future__ import annotations

import pytest

from kagan.ui.screens.troubleshooting import (
    InstallModal,
    IssueType,
    TroubleshootingApp,
    create_no_agents_issues,
)

pytestmark = pytest.mark.unit


class TestInstallModalUnit:
    """Unit tests for InstallModal class."""

    def test_install_modal_initial_state(self):
        """InstallModal initializes with correct default state."""
        modal = InstallModal()

        assert modal._is_installing is False
        assert modal._install_complete is False
        assert modal._install_success is False
        assert modal._result_message == ""

    def test_install_modal_has_bindings(self):
        """InstallModal has correct keybindings."""
        from kagan.keybindings import INSTALL_MODAL_BINDINGS

        assert InstallModal.BINDINGS == INSTALL_MODAL_BINDINGS


class TestTroubleshootingAppNoAgentsCase:
    """Test TroubleshootingApp behavior for no agents case."""

    def test_is_no_agents_case_returns_true_when_all_no_agents(self):
        """_is_no_agents_case returns True when all issues are NO_AGENTS_AVAILABLE."""
        issues = create_no_agents_issues()
        app = TroubleshootingApp(issues)

        assert app._is_no_agents_case() is True

    def test_is_no_agents_case_returns_false_for_other_issues(self):
        """_is_no_agents_case returns False when there are other issue types."""
        from kagan.ui.screens.troubleshooting import (
            ISSUE_PRESETS,
            DetectedIssue,
        )

        issues = [DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])]
        app = TroubleshootingApp(issues)

        assert app._is_no_agents_case() is False

    def test_is_no_agents_case_returns_false_for_mixed_issues(self):
        """_is_no_agents_case returns False for mixed issue types."""
        from kagan.ui.screens.troubleshooting import (
            ISSUE_PRESETS,
            DetectedIssue,
        )

        no_agents_issues = create_no_agents_issues()
        mixed_issues = [
            *no_agents_issues,
            DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING]),
        ]
        app = TroubleshootingApp(mixed_issues)

        assert app._is_no_agents_case() is False


class TestCreateNoAgentsIssues:
    """Test create_no_agents_issues function."""

    def test_creates_issues_for_all_builtin_agents(self):
        """create_no_agents_issues creates an issue for each builtin agent."""
        from kagan.data.builtin_agents import list_builtin_agents

        issues = create_no_agents_issues()
        agents = list_builtin_agents()

        assert len(issues) == len(agents)

    def test_all_issues_are_no_agents_type(self):
        """All created issues have NO_AGENTS_AVAILABLE type."""
        issues = create_no_agents_issues()

        for issue in issues:
            assert issue.preset.type == IssueType.NO_AGENTS_AVAILABLE

    def test_issues_have_install_hints(self):
        """Each issue has an install hint in the hint field."""
        issues = create_no_agents_issues()

        for issue in issues:
            # Install hint should be non-empty
            assert issue.preset.hint
            # Hint should look like an install command (has some content)
            assert len(issue.preset.hint) > 5


class TestInstallModalInstallation:
    """Tests for InstallModal installation logic."""

    async def test_action_cancel_dismisses_with_false(self):
        """action_cancel dismisses the modal with False."""
        modal = InstallModal()
        modal._is_installing = False

        # Mock dismiss
        dismissed_value = None

        def mock_dismiss(value):
            nonlocal dismissed_value
            dismissed_value = value

        modal.dismiss = mock_dismiss  # type: ignore[method-assign]
        modal.action_cancel()

        assert dismissed_value == False  # noqa: E712

    async def test_action_cancel_blocks_during_installation(self):
        """action_cancel shows warning during installation."""
        modal = InstallModal()
        modal._is_installing = True

        # Mock notify and dismiss
        notifications = []

        def mock_notify(msg, severity=None):
            notifications.append((msg, severity))

        def mock_dismiss(value):
            pytest.fail("Should not dismiss during installation")

        modal.notify = mock_notify  # type: ignore[method-assign]
        modal.dismiss = mock_dismiss  # type: ignore[method-assign]
        modal.action_cancel()

        assert len(notifications) == 1
        assert "progress" in notifications[0][0].lower()
        assert notifications[0][1] == "warning"

    async def test_action_confirm_starts_installation_when_not_complete(self, mocker):
        """action_confirm starts installation when not already complete."""
        modal = InstallModal()
        modal._install_complete = False
        modal._is_installing = False

        # Track if action_install was called
        install_called = False

        async def mock_install():
            nonlocal install_called
            install_called = True

        modal.action_install = mock_install
        await modal.action_confirm()

        assert install_called == True  # noqa: E712

    async def test_action_confirm_dismisses_when_complete_and_successful(self):
        """action_confirm dismisses with True when installation was successful."""
        modal = InstallModal()
        modal._install_complete = True
        modal._install_success = True

        dismissed_value = None

        def mock_dismiss(value):
            nonlocal dismissed_value
            dismissed_value = value

        modal.dismiss = mock_dismiss  # type: ignore[method-assign]
        await modal.action_confirm()

        assert dismissed_value == True  # noqa: E712

    async def test_action_confirm_dismisses_with_false_when_complete_but_failed(self):
        """action_confirm dismisses with False when installation failed."""
        modal = InstallModal()
        modal._install_complete = True
        modal._install_success = False

        dismissed_value = None

        def mock_dismiss(value):
            nonlocal dismissed_value
            dismissed_value = value

        modal.dismiss = mock_dismiss  # type: ignore[method-assign]
        await modal.action_confirm()

        assert dismissed_value == False  # noqa: E712
