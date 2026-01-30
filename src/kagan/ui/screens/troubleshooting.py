"""Troubleshooting screen shown for pre-flight check failures."""

from __future__ import annotations

import platform
import shlex
import shutil
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import TYPE_CHECKING

from textual.app import App, ComposeResult
from textual.containers import Center, Container, Middle, VerticalScroll
from textual.widgets import Footer, Static

from kagan.constants import KAGAN_LOGO
from kagan.keybindings import TROUBLESHOOTING_BINDINGS, to_textual_bindings
from kagan.theme import KAGAN_THEME
from kagan.ui.utils.clipboard import copy_with_notification

if TYPE_CHECKING:
    from textual.events import Click

    from kagan.config import AgentConfig


class IssueType(Enum):
    """Types of pre-flight issues."""

    WINDOWS_OS = "windows_os"
    INSTANCE_LOCKED = "instance_locked"
    TMUX_MISSING = "tmux_missing"
    AGENT_MISSING = "agent_missing"
    NPX_MISSING = "npx_missing"
    ACP_AGENT_MISSING = "acp_agent_missing"


class IssueSeverity(Enum):
    """Severity levels for issues."""

    BLOCKING = "blocking"
    WARNING = "warning"


@dataclass(frozen=True)
class IssuePreset:
    """Predefined issue configuration."""

    type: IssueType
    severity: IssueSeverity
    icon: str
    title: str
    message: str
    hint: str
    url: str | None = None


# Predefined issue messages
ISSUE_PRESETS: dict[IssueType, IssuePreset] = {
    IssueType.WINDOWS_OS: IssuePreset(
        type=IssueType.WINDOWS_OS,
        severity=IssueSeverity.BLOCKING,
        icon="[!]",
        title="Windows Not Supported",
        message=(
            "Kagan does not currently support Windows.\n"
            "We recommend using WSL2 (Windows Subsystem for Linux)."
        ),
        hint="Install WSL2 and run Kagan from there",
        url="https://github.com/aorumbayev/kagan",
    ),
    IssueType.INSTANCE_LOCKED: IssuePreset(
        type=IssueType.INSTANCE_LOCKED,
        severity=IssueSeverity.BLOCKING,
        icon="[!]",
        title="Another Instance Running",
        message=(
            "Another Kagan instance is already running in this folder.\n"
            "Please return to that window or close it before starting."
        ),
        hint="Close the other instance and try again",
    ),
    IssueType.TMUX_MISSING: IssuePreset(
        type=IssueType.TMUX_MISSING,
        severity=IssueSeverity.BLOCKING,
        icon="[!]",
        title="tmux Not Available",
        message=(
            "tmux is required for PAIR mode but was not found in PATH.\n"
            "PAIR mode uses tmux for interactive agent sessions."
        ),
        hint="Install tmux: brew install tmux (macOS) or apt install tmux (Linux)",
    ),
    IssueType.AGENT_MISSING: IssuePreset(
        type=IssueType.AGENT_MISSING,
        severity=IssueSeverity.BLOCKING,
        icon="[!]",
        title="Default Agent Not Installed",
        message="The default agent was not found in PATH.",
        hint="Install the agent to continue",
    ),
    IssueType.NPX_MISSING: IssuePreset(
        type=IssueType.NPX_MISSING,
        severity=IssueSeverity.BLOCKING,
        icon="[!]",
        title="npx Not Available",
        message=(
            "npx is required to run claude-code-acp but was not found.\n"
            "Either install Node.js (which includes npx) or install\n"
            "claude-code-acp globally."
        ),
        hint=(
            "Option 1: Install Node.js from https://nodejs.org\n"
            "Option 2: npm install -g @zed-industries/claude-code-acp"
        ),
        url="https://github.com/zed-industries/claude-code-acp",
    ),
    IssueType.ACP_AGENT_MISSING: IssuePreset(
        type=IssueType.ACP_AGENT_MISSING,
        severity=IssueSeverity.BLOCKING,
        icon="[!]",
        title="ACP Agent Not Available",
        message=(
            "The ACP agent executable was not found.\n"
            "Neither npx nor a global installation is available."
        ),
        hint="Install the agent globally or ensure npx is available",
    ),
}


@dataclass(frozen=True)
class DetectedIssue:
    """A detected pre-flight issue with optional runtime details."""

    preset: IssuePreset
    details: str | None = None


@dataclass
class PreflightResult:
    """Result of pre-flight checks."""

    issues: list[DetectedIssue]

    @property
    def has_blocking_issues(self) -> bool:
        """Check if any blocking issues were detected."""
        return any(issue.preset.severity == IssueSeverity.BLOCKING for issue in self.issues)


@dataclass
class ACPCommandResolution:
    """Result of resolving an ACP command.

    This handles the case where a command like "npx claude-code-acp" needs
    to be resolved to either:
    1. Use npx if available
    2. Use the global binary (claude-code-acp) if installed globally
    3. Report an error if neither is available
    """

    resolved_command: str | None
    issue: DetectedIssue | None
    used_fallback: bool = False


def _is_npx_command(command: str) -> bool:
    """Check if a command uses npx."""
    try:
        parts = shlex.split(command)
        return len(parts) > 0 and parts[0] == "npx"
    except ValueError:
        return command.startswith("npx ")


def _get_npx_package_binary(command: str) -> str | None:
    """Extract the binary name from an npx command.

    For "npx claude-code-acp", returns "claude-code-acp".
    For "npx @anthropic-ai/claude-code-acp", returns "claude-code-acp".
    """
    try:
        parts = shlex.split(command)
        if len(parts) < 2:
            return None
        package = parts[1]
        # Handle scoped packages like @anthropic-ai/claude-code-acp
        if "/" in package:
            return package.split("/")[-1]
        return package
    except ValueError:
        return None


def resolve_acp_command(
    run_command: str,
    agent_name: str = "Claude Code",
) -> ACPCommandResolution:
    """Resolve an ACP command, handling npx fallback scenarios.

    Logic:
    1. If the command uses npx (e.g., "npx claude-code-acp"):
       a. If the binary is globally installed, use it directly
       b. Else if npx is available, use the original npx command
       c. Else report an error with installation instructions
    2. If the command doesn't use npx, check if the binary exists

    Args:
        run_command: The configured ACP run command (e.g., "npx claude-code-acp")
        agent_name: Display name for error messages

    Returns:
        ACPCommandResolution with the resolved command or an issue
    """
    if _is_npx_command(run_command):
        binary_name = _get_npx_package_binary(run_command)
        if binary_name is None:
            # Malformed npx command
            preset = IssuePreset(
                type=IssueType.ACP_AGENT_MISSING,
                severity=IssueSeverity.BLOCKING,
                icon="[!]",
                title="Invalid ACP Command",
                message=f"The ACP command '{run_command}' appears to be malformed.",
                hint="Check your agent configuration",
            )
            return ACPCommandResolution(
                resolved_command=None,
                issue=DetectedIssue(preset=preset, details=agent_name),
            )

        # Check if the binary is globally installed
        if shutil.which(binary_name) is not None:
            # Great! Use the global binary directly
            # Preserve any additional args from the original command
            try:
                parts = shlex.split(run_command)
                # Replace "npx <package>" with just "<binary>"
                resolved = " ".join([binary_name, *parts[2:]])
            except ValueError:
                resolved = binary_name
            return ACPCommandResolution(
                resolved_command=resolved,
                issue=None,
                used_fallback=True,
            )

        # Check if npx is available
        if shutil.which("npx") is not None:
            # Use the original npx command
            return ACPCommandResolution(
                resolved_command=run_command,
                issue=None,
                used_fallback=False,
            )

        # Neither global binary nor npx is available
        preset = IssuePreset(
            type=IssueType.NPX_MISSING,
            severity=IssueSeverity.BLOCKING,
            icon="[!]",
            title="npx Not Available",
            message=(
                f"The {agent_name} ACP agent requires npx or a global installation.\n"
                f"npx was not found and '{binary_name}' is not installed globally."
            ),
            hint=(
                f"Option 1: Install Node.js from https://nodejs.org (includes npx)\n"
                f"Option 2: npm install -g {binary_name}"
            ),
            url="https://github.com/zed-industries/claude-code-acp",
        )
        return ACPCommandResolution(
            resolved_command=None,
            issue=DetectedIssue(preset=preset, details=agent_name),
        )

    # Non-npx command: just check if the executable exists
    try:
        parts = shlex.split(run_command)
        executable = parts[0] if parts else run_command
    except ValueError:
        executable = run_command

    if shutil.which(executable) is not None:
        return ACPCommandResolution(
            resolved_command=run_command,
            issue=None,
            used_fallback=False,
        )

    # Executable not found
    preset = IssuePreset(
        type=IssueType.ACP_AGENT_MISSING,
        severity=IssueSeverity.BLOCKING,
        icon="[!]",
        title=f"{agent_name} ACP Agent Not Found",
        message=f"The ACP agent executable '{executable}' was not found in PATH.",
        hint=f"Ensure '{executable}' is installed and available in PATH",
    )
    return ACPCommandResolution(
        resolved_command=None,
        issue=DetectedIssue(preset=preset, details=agent_name),
    )


def _check_windows() -> DetectedIssue | None:
    """Check if running on Windows."""
    if platform.system() == "Windows":
        return DetectedIssue(preset=ISSUE_PRESETS[IssueType.WINDOWS_OS])
    return None


def _check_tmux() -> DetectedIssue | None:
    """Check if tmux is available."""
    if shutil.which("tmux") is None:
        return DetectedIssue(preset=ISSUE_PRESETS[IssueType.TMUX_MISSING])
    return None


def _check_agent(
    agent_command: str,
    agent_name: str,
    install_command: str | None,
) -> DetectedIssue | None:
    """Check if the configured agent is available."""
    # Parse command to get the executable
    try:
        parts = shlex.split(agent_command)
        executable = parts[0] if parts else agent_command
    except ValueError:
        executable = agent_command

    if shutil.which(executable) is None:
        # Create a customized preset with agent-specific details
        preset = IssuePreset(
            type=IssueType.AGENT_MISSING,
            severity=IssueSeverity.BLOCKING,
            icon="[!]",
            title="Default Agent Not Installed",
            message=f"The default agent ({agent_name}) was not found in PATH.",
            hint=(
                f"Install: {install_command}"
                if install_command
                else f"Ensure '{executable}' is available in PATH"
            ),
        )
        return DetectedIssue(preset=preset, details=agent_name)
    return None


def detect_issues(
    *,
    check_lock: bool = False,
    lock_acquired: bool = True,
    agent_config: AgentConfig | None = None,
    agent_name: str = "Claude Code",
    agent_install_command: str | None = None,
) -> PreflightResult:
    """Run all pre-flight checks and return detected issues.

    Args:
        check_lock: Whether to check instance lock status.
        lock_acquired: If check_lock is True, whether the lock was acquired.
        agent_config: Optional agent configuration to check.
        agent_name: Display name of the agent to check.
        agent_install_command: Installation command for the agent.

    Returns:
        PreflightResult containing all detected issues.
    """
    issues: list[DetectedIssue] = []

    # 1. Windows check (exit early - nothing else matters)
    windows_issue = _check_windows()
    if windows_issue:
        return PreflightResult(issues=[windows_issue])

    # 2. Instance lock check
    if check_lock and not lock_acquired:
        issues.append(DetectedIssue(preset=ISSUE_PRESETS[IssueType.INSTANCE_LOCKED]))

    # 3. tmux check
    tmux_issue = _check_tmux()
    if tmux_issue:
        issues.append(tmux_issue)

    # 4. Agent check (interactive command for PAIR mode)
    if agent_config:
        from kagan.config import get_os_value

        interactive_cmd = get_os_value(agent_config.interactive_command)
        if interactive_cmd:
            agent_issue = _check_agent(
                agent_command=interactive_cmd,
                agent_name=agent_name,
                install_command=agent_install_command,
            )
            if agent_issue:
                issues.append(agent_issue)

        # 5. ACP command check (run_command for AUTO mode)
        # This uses smart detection for npx-based commands
        acp_cmd = get_os_value(agent_config.run_command)
        if acp_cmd:
            acp_resolution = resolve_acp_command(acp_cmd, agent_name)
            if acp_resolution.issue:
                issues.append(acp_resolution.issue)

    return PreflightResult(issues=issues)


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
    """Standalone app shown when pre-flight checks fail."""

    TITLE = "KAGAN"
    CSS_PATH = str(Path(__file__).resolve().parents[2] / "styles" / "kagan.tcss")

    BINDINGS = to_textual_bindings(TROUBLESHOOTING_BINDINGS)

    def __init__(self, issues: list[DetectedIssue]) -> None:
        super().__init__()
        self._issues = issues
        self.register_theme(KAGAN_THEME)
        self.theme = "kagan"

    def compose(self) -> ComposeResult:
        blocking_count = sum(
            1 for issue in self._issues if issue.preset.severity == IssueSeverity.BLOCKING
        )

        with Container(id="troubleshoot-container"):
            with Middle():
                with Center():
                    with Static(id="troubleshoot-card"):
                        yield Static(KAGAN_LOGO, id="troubleshoot-logo")
                        yield Static("Startup Issues Detected", id="troubleshoot-title")
                        plural = "s" if blocking_count != 1 else ""
                        yield Static(
                            f"{blocking_count} blocking issue{plural} found",
                            id="troubleshoot-count",
                        )
                        with VerticalScroll(id="troubleshoot-issues"):
                            for issue in self._issues:
                                with Container(classes="issue-card"):
                                    yield IssueCard(issue)
                        yield Static(
                            "Resolve issues and restart Kagan",
                            id="troubleshoot-resolve-hint",
                        )
                        yield Static("Press q to exit", id="troubleshoot-exit-hint")
        yield Footer()
