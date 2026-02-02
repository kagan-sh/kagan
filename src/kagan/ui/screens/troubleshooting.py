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
from textual.binding import Binding, BindingType
from textual.containers import Center, Container, Middle, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Footer, Label, LoadingIndicator, Select, Static
from textual.widgets._select import NoSelection

from kagan.constants import KAGAN_LOGO
from kagan.keybindings import INSTALL_MODAL_BINDINGS, TROUBLESHOOTING_BINDINGS
from kagan.terminal import supports_truecolor
from kagan.theme import KAGAN_THEME, KAGAN_THEME_256
from kagan.ui.utils.clipboard import copy_with_notification

if TYPE_CHECKING:
    from textual.events import Click

    from kagan.config import AgentConfig

# Bindings for the agent selection modal
AGENT_SELECT_MODAL_BINDINGS: list[BindingType] = [
    Binding("escape", "cancel", "Cancel"),
    Binding("enter", "select", "Select"),
]


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
    TERMINAL_NO_TRUECOLOR = "terminal_no_truecolor"  # Terminal doesn't support truecolor


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
        severity=IssueSeverity.WARNING,
        icon="[~]",
        title="tmux Not Installed",
        message=(
            "tmux is not installed.\n\n"
            "PAIR mode (collaborative sessions) requires tmux.\n"
            "AUTO mode will work normally without it."
        ),
        hint="To install: brew install tmux (macOS) or apt install tmux (Linux)",
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
    IssueType.TERMINAL_NO_TRUECOLOR: IssuePreset(
        type=IssueType.TERMINAL_NO_TRUECOLOR,
        severity=IssueSeverity.WARNING,
        icon="[~]",
        title="Using 256-Color Fallback Theme",
        message=(
            "Your terminal doesn't appear to support truecolor (24-bit colors).\n"
            "Kagan is using a 256-color fallback theme for better compatibility."
        ),
        hint=(
            "For optimal colors, use a truecolor terminal:\n"
            "  • iTerm2, Warp, Kitty, Ghostty, or VS Code terminal\n"
            "  • Or set: export COLORTERM=truecolor"
        ),
        url="https://github.com/aorumbayev/kagan#terminal-requirements",
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


def _check_terminal_truecolor() -> DetectedIssue | None:
    """Check if terminal supports truecolor (24-bit colors)."""
    from kagan.terminal import get_terminal_name, supports_truecolor

    if not supports_truecolor():
        terminal_name = get_terminal_name()
        preset = IssuePreset(
            type=IssueType.TERMINAL_NO_TRUECOLOR,
            severity=IssueSeverity.WARNING,
            icon="[~]",
            title="Using 256-Color Fallback Theme",
            message=(
                f"Your terminal ({terminal_name}) doesn't appear to support truecolor.\n"
                "Kagan is using a 256-color fallback theme for better compatibility."
            ),
            hint=(
                "For optimal colors, use a truecolor terminal:\n"
                "  • iTerm2, Warp, Kitty, Ghostty, or VS Code terminal\n"
                "  • Or set: export COLORTERM=truecolor"
            ),
            url="https://github.com/aorumbayev/kagan#terminal-requirements",
        )
        return DetectedIssue(preset=preset, details=terminal_name)
    return None


async def detect_issues(
    *,
    check_lock: bool = False,
    lock_acquired: bool = True,
    agent_config: AgentConfig | None = None,
    agent_name: str = "Claude Code",
    agent_install_command: str | None = None,
    check_git: bool = True,
    check_terminal: bool = True,
) -> PreflightResult:
    """Run all pre-flight checks and return detected issues.

    Args:
        check_lock: Whether to check instance lock status.
        lock_acquired: If check_lock is True, whether the lock was acquired.
        agent_config: Optional agent configuration to check.
        agent_name: Display name of the agent to check.
        agent_install_command: Installation command for the agent.
        check_git: Whether to check git version and configuration.
        check_terminal: Whether to check terminal truecolor support.

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

    # 5. Terminal truecolor check (warning only)
    if check_terminal:
        terminal_issue = _check_terminal_truecolor()
        if terminal_issue:
            issues.append(terminal_issue)

    # 6. Agent check (interactive command for PAIR mode)
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

        # 7. ACP command check (run_command for AUTO mode)
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


class AgentSelectModal(ModalScreen[str | None]):
    """Modal for selecting which agent to install."""

    BINDINGS = AGENT_SELECT_MODAL_BINDINGS

    def __init__(self, agents: list[str], **kwargs) -> None:
        super().__init__(**kwargs)
        self._agents = agents

    def compose(self) -> ComposeResult:
        from kagan.data.builtin_agents import get_builtin_agent

        with Container(id="agent-select-modal"):
            yield Label("Select Agent to Install", classes="install-modal-title")
            options: list[tuple[str, str]] = []
            for agent_id in self._agents:
                info = get_builtin_agent(agent_id)
                name = info.config.name if info else agent_id.title()
                options.append((name, agent_id))
            yield Select[str](
                options,
                id="agent-select",
                value=self._agents[0] if self._agents else NoSelection(),
            )
            yield Label(
                "Press Enter to select, Escape to cancel",
                classes="install-modal-hint",
            )
        yield Footer()

    def action_select(self) -> None:
        """Select the chosen agent."""
        select = self.query_one("#agent-select", Select)
        self.dismiss(str(select.value) if select.value else None)

    def action_cancel(self) -> None:
        """Cancel and close the modal."""
        self.dismiss(None)


class InstallModal(ModalScreen[bool]):
    """Modal for installing an AI agent."""

    BINDINGS = INSTALL_MODAL_BINDINGS

    def __init__(self, agent: str = "claude", **kwargs) -> None:
        super().__init__(**kwargs)
        self._agent = agent
        self._is_installing = False
        self._install_complete = False
        self._install_success = False
        self._result_message = ""

    def compose(self) -> ComposeResult:
        from kagan.agents.installer import get_install_command
        from kagan.data.builtin_agents import get_builtin_agent

        agent_info = get_builtin_agent(self._agent)
        agent_name = agent_info.config.name if agent_info else self._agent.title()
        install_cmd = get_install_command(self._agent)

        with Container(id="install-modal-container"):
            yield Label(f"Install {agent_name}", classes="install-modal-title")
            yield Label(
                "This will run the installation command:",
                classes="install-modal-subtitle",
            )
            yield Label(
                f"$ {install_cmd}",
                id="install-command",
                classes="install-modal-command",
            )
            yield LoadingIndicator(id="install-spinner")
            yield Label("", id="install-status", classes="install-modal-status")
            yield Label(
                "Press Enter to install, Escape to cancel",
                id="install-hint",
                classes="install-modal-hint",
            )
        yield Footer()

    def on_mount(self) -> None:
        """Hide spinner initially."""
        self.query_one("#install-spinner", LoadingIndicator).display = False

    async def action_install(self) -> None:
        """Start the installation process."""
        if self._is_installing or self._install_complete:
            return

        from kagan.data.builtin_agents import get_builtin_agent

        self._is_installing = True
        spinner = self.query_one("#install-spinner", LoadingIndicator)
        status = self.query_one("#install-status", Label)
        hint = self.query_one("#install-hint", Label)

        # Get agent name for display
        agent_info = get_builtin_agent(self._agent)
        agent_name = agent_info.config.name if agent_info else self._agent.title()

        # Show spinner and update status
        spinner.display = True
        status.update(f"Installing {agent_name}...")
        hint.update("Please wait...")

        # Run installation
        try:
            from kagan.agents.installer import install_agent

            success, message = await install_agent(self._agent)
            self._install_success = success
            self._result_message = message
        except Exception as e:
            self._install_success = False
            self._result_message = f"Installation error: {e}"

        # Hide spinner and show result
        spinner.display = False
        self._install_complete = True
        self._is_installing = False

        if self._install_success:
            status.add_class("success")
            status.update(f"[bold green]Success![/] {self._result_message}")
            hint.update("Press Enter to restart Kagan, Escape to close")
        else:
            status.add_class("error")
            status.update(f"[bold red]Failed:[/] {self._result_message}")
            hint.update("Press Escape to close")

    async def action_confirm(self) -> None:
        """Confirm action - install or dismiss with success."""
        if self._install_complete:
            # If installation is complete and successful, dismiss with True to signal restart
            self.dismiss(self._install_success)
        else:
            # Start installation
            await self.action_install()

    def action_cancel(self) -> None:
        """Cancel and close the modal."""
        if self._is_installing:
            # Can't cancel during installation
            self.notify("Installation in progress...", severity="warning")
            return
        self.dismiss(False)


class TroubleshootingApp(App):
    """Standalone app shown when pre-flight checks fail or have warnings."""

    TITLE = "KAGAN"
    CSS_PATH = str(Path(__file__).resolve().parents[2] / "styles" / "kagan.tcss")

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
            resolve_hint = "Install an agent and restart Kagan"
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
        yield Footer()

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
        from kagan.data.builtin_agents import list_builtin_agents

        agents = [a.config.short_name for a in list_builtin_agents()]

        if len(agents) == 1:
            # Only one option, skip selection
            self._show_install_modal(agents[0])
        else:
            # Show selection modal first
            def handle_selection(agent: str | None) -> None:
                if agent:
                    self._show_install_modal(agent)

            self.push_screen(AgentSelectModal(agents), handle_selection)

    def _show_install_modal(self, agent: str) -> None:
        """Show the install modal for a specific agent."""

        def handle_install_result(result: bool | None) -> None:
            if result:
                self.notify("Installation complete! Please restart Kagan.", severity="information")
                # Exit the app so user can restart
                self.exit(0)

        self.push_screen(InstallModal(agent=agent), handle_install_result)
