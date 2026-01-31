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
    NO_AGENTS_AVAILABLE = "no_agents_available"  # No supported agents installed
    GIT_VERSION_LOW = "git_version_low"  # Git version too old for worktrees
    GIT_USER_NOT_CONFIGURED = "git_user_not_configured"  # Git user.name/email not set
    GIT_NOT_INSTALLED = "git_not_installed"  # Git is not installed


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
    IssueType.GIT_NOT_INSTALLED: IssuePreset(
        type=IssueType.GIT_NOT_INSTALLED,
        severity=IssueSeverity.BLOCKING,
        icon="[!]",
        title="Git Not Installed",
        message=(
            "Git is required but was not found on your system.\n"
            "Kagan uses git worktrees for isolated development environments."
        ),
        hint="Install Git: brew install git (macOS) or apt install git (Linux)",
        url="https://git-scm.com/downloads",
    ),
    IssueType.GIT_VERSION_LOW: IssuePreset(
        type=IssueType.GIT_VERSION_LOW,
        severity=IssueSeverity.BLOCKING,
        icon="[!]",
        title="Git Version Too Old",
        message=(
            "Your Git version does not support worktrees.\n"
            "Kagan requires Git 2.5 or later for worktree functionality."
        ),
        hint="Upgrade Git: brew upgrade git (macOS) or apt update && apt upgrade git (Linux)",
        url="https://git-scm.com/downloads",
    ),
    IssueType.GIT_USER_NOT_CONFIGURED: IssuePreset(
        type=IssueType.GIT_USER_NOT_CONFIGURED,
        severity=IssueSeverity.BLOCKING,
        icon="[!]",
        title="Git User Not Configured",
        message=(
            "Git user identity is not configured.\nKagan needs to make commits to track changes."
        ),
        hint=(
            "Run:\n"
            '  git config --global user.name "Your Name"\n'
            '  git config --global user.email "your@email.com"'
        ),
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


async def _check_git_version() -> DetectedIssue | None:
    """Check if git is installed and version supports worktrees."""
    from kagan.git_utils import MIN_GIT_VERSION, get_git_version

    version = await get_git_version()
    if version is None:
        return DetectedIssue(preset=ISSUE_PRESETS[IssueType.GIT_NOT_INSTALLED])

    if version < MIN_GIT_VERSION:
        # Create a customized preset with version details
        preset = IssuePreset(
            type=IssueType.GIT_VERSION_LOW,
            severity=IssueSeverity.BLOCKING,
            icon="[!]",
            title="Git Version Too Old",
            message=(
                f"Your Git version ({version}) does not support worktrees.\n"
                f"Kagan requires Git {MIN_GIT_VERSION[0]}.{MIN_GIT_VERSION[1]}+ "
                "for worktree functionality."
            ),
            hint="Upgrade Git: brew upgrade git (macOS) or apt update && apt upgrade git (Linux)",
            url="https://git-scm.com/downloads",
        )
        return DetectedIssue(preset=preset, details=str(version))

    return None


async def _check_git_user() -> DetectedIssue | None:
    """Check if git user.name and user.email are configured."""
    from kagan.git_utils import check_git_user_configured

    is_configured, error_msg = await check_git_user_configured()
    if not is_configured:
        # Create a customized preset with specific error
        preset = IssuePreset(
            type=IssueType.GIT_USER_NOT_CONFIGURED,
            severity=IssueSeverity.BLOCKING,
            icon="[!]",
            title="Git User Not Configured",
            message=(f"{error_msg}\nKagan needs to make commits to track changes."),
            hint=(
                "Run:\n"
                '  git config --global user.name "Your Name"\n'
                '  git config --global user.email "your@email.com"'
            ),
        )
        return DetectedIssue(preset=preset, details=error_msg)

    return None


async def detect_issues(
    *,
    check_lock: bool = False,
    lock_acquired: bool = True,
    agent_config: AgentConfig | None = None,
    agent_name: str = "Claude Code",
    agent_install_command: str | None = None,
    check_git: bool = True,
) -> PreflightResult:
    """Run all pre-flight checks and return detected issues.

    Args:
        check_lock: Whether to check instance lock status.
        lock_acquired: If check_lock is True, whether the lock was acquired.
        agent_config: Optional agent configuration to check.
        agent_name: Display name of the agent to check.
        agent_install_command: Installation command for the agent.
        check_git: Whether to check git version and configuration.

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

    # 3. Git checks (version and user configuration)
    if check_git:
        git_version_issue = await _check_git_version()
        if git_version_issue:
            issues.append(git_version_issue)
        else:
            # Only check user config if git is installed and version is OK
            git_user_issue = await _check_git_user()
            if git_user_issue:
                issues.append(git_user_issue)

    # 4. tmux check
    tmux_issue = _check_tmux()
    if tmux_issue:
        issues.append(tmux_issue)

    # 5. Agent check (interactive command for PAIR mode)
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

        # 6. ACP command check (run_command for AUTO mode)
        # This uses smart detection for npx-based commands
        acp_cmd = get_os_value(agent_config.run_command)
        if acp_cmd:
            acp_resolution = resolve_acp_command(acp_cmd, agent_name)
            if acp_resolution.issue:
                issues.append(acp_resolution.issue)

    return PreflightResult(issues=issues)


def create_no_agents_issues() -> list[DetectedIssue]:
    """Create issues showing all available agent install options.

    Used when no supported agents are found on the system. Each agent
    gets its own issue card with installation instructions.

    Returns:
        List of DetectedIssue for each supported agent.
    """
    from kagan.data.builtin_agents import list_builtin_agents

    issues = []
    for agent in list_builtin_agents():
        preset = IssuePreset(
            type=IssueType.NO_AGENTS_AVAILABLE,
            severity=IssueSeverity.BLOCKING,
            icon="[+]",  # Plus icon for "install me"
            title=f"Install {agent.config.name}",
            message=f"{agent.description}\nBy {agent.author}",
            hint=agent.install_command,
            url=agent.docs_url if agent.docs_url else None,
        )
        issues.append(DetectedIssue(preset=preset))

    return issues


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

    def _is_no_agents_case(self) -> bool:
        """Check if this is the 'no agents available' case."""
        return all(issue.preset.type == IssueType.NO_AGENTS_AVAILABLE for issue in self._issues)

    def compose(self) -> ComposeResult:
        blocking_count = sum(
            1 for issue in self._issues if issue.preset.severity == IssueSeverity.BLOCKING
        )

        # Determine title and subtitle based on issue type
        is_no_agents = self._is_no_agents_case()
        if is_no_agents:
            title = "No AI Agents Found"
            subtitle = "Install one of the following to get started:"
            resolve_hint = "Install an agent and restart Kagan"
        else:
            title = "Startup Issues Detected"
            plural = "s" if blocking_count != 1 else ""
            subtitle = f"{blocking_count} blocking issue{plural} found"
            resolve_hint = "Resolve issues and restart Kagan"

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
                        yield Static("Press q to exit", id="troubleshoot-exit-hint")
        yield Footer()
