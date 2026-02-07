"""Troubleshooting screen shown for pre-flight check failures."""

from __future__ import annotations

from importlib import resources
from typing import TYPE_CHECKING, cast

from textual.app import App
from textual.containers import Center, Container, Middle, VerticalScroll
from textual.widgets import Footer, Static

from kagan.constants import KAGAN_LOGO
from kagan.keybindings import TROUBLESHOOTING_BINDINGS
from kagan.terminal import supports_truecolor
from kagan.theme import KAGAN_THEME, KAGAN_THEME_256
from kagan.ui.screens.troubleshooting.issue_presets import DetectedIssue, IssueSeverity, IssueType
from kagan.ui.screens.troubleshooting.modals import AgentSelectModal, InstallModal
from kagan.ui.utils.clipboard import copy_with_notification

if TYPE_CHECKING:
    from textual.app import ComposeResult
    from textual.events import Click

    from kagan.agents.installer import AgentType


class CopyableHint(Static):
    """Hint text that copies on single-click."""

    DEFAULT_CLASSES = "issue-card-hint"

    def __init__(self, hint: str) -> None:
        super().__init__(f"Hint: {hint}")
        self._hint = hint

    async def _on_click(self, event: Click) -> None:
        """Copy hint text on single-click."""
        copy_with_notification(self.app, self._hint, "Hint")


class CopyableUrl(Static):
    """URL that copies on single-click."""

    DEFAULT_CLASSES = "issue-card-url"

    def __init__(self, url: str) -> None:
        super().__init__(f"More info: {url}")
        self._url = url

    async def _on_click(self, event: Click) -> None:
        """Copy URL on single-click."""
        copy_with_notification(self.app, self._url, "URL")


class IssueCard(Static):
    """Widget displaying a single issue."""

    def __init__(self, issue: DetectedIssue) -> None:
        super().__init__()
        self._issue = issue

    def compose(self) -> ComposeResult:
        preset = self._issue.preset
        yield Static(f"{preset.icon} {preset.title}", classes="issue-card-title")
        yield Static(preset.message, classes="issue-card-message")
        yield CopyableHint(preset.hint)
        if preset.url:
            yield CopyableUrl(preset.url)


class TroubleshootingApp(App):
    """Standalone app shown when pre-flight checks fail or have warnings."""

    TITLE = "KAGAN"
    CSS_PATH = str(resources.files("kagan.styles") / "kagan.tcss")

    BINDINGS = TROUBLESHOOTING_BINDINGS

    # Exit codes for different outcomes
    EXIT_QUIT = 1
    EXIT_CONTINUE = 0

    def __init__(self, issues: list[DetectedIssue]) -> None:
        super().__init__()
        self._issues = issues
        # Register both themes and select based on terminal capabilities
        self.register_theme(KAGAN_THEME)
        self.register_theme(KAGAN_THEME_256)
        if supports_truecolor():
            self.theme = "kagan"
        else:
            self.theme = "kagan-256"

    def _is_no_agents_case(self) -> bool:
        """Check if this is the 'no agents available' case."""
        return all(issue.preset.type == IssueType.NO_AGENTS_AVAILABLE for issue in self._issues)

    def _has_only_warnings(self) -> bool:
        """Check if all issues are warnings (no blocking issues)."""
        return all(issue.preset.severity == IssueSeverity.WARNING for issue in self._issues)

    def compose(self) -> ComposeResult:
        blocking_count = sum(
            1 for issue in self._issues if issue.preset.severity == IssueSeverity.BLOCKING
        )
        warning_count = sum(
            1 for issue in self._issues if issue.preset.severity == IssueSeverity.WARNING
        )

        # Determine title and subtitle based on issue type
        is_no_agents = self._is_no_agents_case()
        has_only_warnings = self._has_only_warnings()

        if is_no_agents:
            title = "No AI Agents Found"
            subtitle = "Install one of the following to get started:"
            resolve_hint = (
                "Install an agent and restart Kagan\n"
                "or run 'kagan --skip-preflight' to continue in limited mode"
            )
            exit_hint = "i = Install Agent | q = Quit"
        elif has_only_warnings:
            title = "Startup Warnings"
            plural = "s" if warning_count != 1 else ""
            subtitle = f"{warning_count} warning{plural} detected"
            resolve_hint = "You can continue, but some features may not work optimally"
            exit_hint = "Enter/c = Continue | q = Quit"
        else:
            title = "Startup Issues Detected"
            plural = "s" if blocking_count != 1 else ""
            subtitle = f"{blocking_count} blocking issue{plural} found"
            resolve_hint = "Resolve issues and restart Kagan"
            exit_hint = "Press q to exit"

        with Container(id="troubleshoot-container"):
            with Middle():
                with Center():
                    with Static(id="troubleshoot-card"):
                        yield Static(KAGAN_LOGO, id="troubleshoot-logo")
                        yield Static(title, id="troubleshoot-title")
                        yield Static(subtitle, id="troubleshoot-count")
                        with VerticalScroll(id="troubleshoot-issues"):
                            for issue in self._issues:
                                with Container(classes="issue-card"):
                                    yield IssueCard(issue)
                        yield Static(resolve_hint, id="troubleshoot-resolve-hint")
                        yield Static(exit_hint, id="troubleshoot-exit-hint")
        yield Footer(show_command_palette=False)

    def action_continue_app(self) -> None:
        """Continue to the main app (only for warning-only cases)."""
        if self._has_only_warnings():
            self.exit(self.EXIT_CONTINUE)
        else:
            self.notify("Cannot continue - resolve blocking issues first", severity="error")

    def action_install_agent(self) -> None:
        """Open the agent selection then install modal."""
        if not self._is_no_agents_case():
            self.notify(
                "Install option only available when no agents are found", severity="warning"
            )
            return

        # Get list of installable agents
        from kagan.builtin_agents import list_builtin_agents

        agents = [a.config.short_name for a in list_builtin_agents()]

        if len(agents) == 1:
            # Only one option, skip selection
            self._show_install_modal(cast("AgentType", agents[0]))
        else:
            # Show selection modal first
            def handle_selection(agent: str | None) -> None:
                if agent:
                    self._show_install_modal(cast("AgentType", agent))

            self.push_screen(AgentSelectModal(agents), handle_selection)

    def _show_install_modal(self, agent: AgentType) -> None:
        """Show the install modal for a specific agent."""

        def handle_install_result(result: bool | None) -> None:
            if result:
                self.notify("Installation complete! Please restart Kagan.", severity="information")
                # Exit the app so user can restart
                self.exit(0)

        self.push_screen(InstallModal(agent=agent), handle_install_result)
