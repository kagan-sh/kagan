"""Pre-flight checks and issue detection for Kagan startup.

Pure detection logic with no TUI dependencies. Used by both the CLI
entry point (__main__.py) and the ACP agent (acp/kagan_agent.py).
"""

from __future__ import annotations

import platform
import shutil
from dataclasses import dataclass
from enum import Enum
from typing import TYPE_CHECKING

from kagan.command_utils import split_command_string

if TYPE_CHECKING:
    from kagan.config import AgentConfig


class IssueType(Enum):
    WINDOWS_OS = "windows_os"
    TMUX_MISSING = "tmux_missing"
    WEZTERM_MISSING = "wezterm_missing"
    AGENT_MISSING = "agent_missing"
    NPX_MISSING = "npx_missing"
    ACP_AGENT_MISSING = "acp_agent_missing"
    NO_AGENTS_AVAILABLE = "no_agents_available"
    GIT_VERSION_LOW = "git_version_low"
    GIT_USER_NOT_CONFIGURED = "git_user_not_configured"
    GIT_NOT_INSTALLED = "git_not_installed"
    TERMINAL_NO_TRUECOLOR = "terminal_no_truecolor"


class IssueSeverity(Enum):
    BLOCKING = "blocking"
    WARNING = "warning"


@dataclass(frozen=True)
class IssuePreset:
    type: IssueType
    severity: IssueSeverity
    icon: str
    title: str
    message: str
    hint: str
    url: str | None = None


ISSUE_PRESETS: dict[IssueType, IssuePreset] = {
    IssueType.WINDOWS_OS: IssuePreset(
        type=IssueType.WINDOWS_OS,
        severity=IssueSeverity.WARNING,
        icon="[~]",
        title="Windows Compatibility Notes",
        message=(
            "Kagan runs on Windows, but PAIR mode relies on tmux.\n"
            "Use AUTO mode for a no-tmux workflow, or run via WSL2 for full PAIR support."
        ),
        hint="Recommended: start with AUTO tickets on Windows. Use WSL2 for tmux PAIR sessions.",
        url="https://github.com/aorumbayev/kagan",
    ),
    IssueType.TMUX_MISSING: IssuePreset(
        type=IssueType.TMUX_MISSING,
        severity=IssueSeverity.WARNING,
        icon="[~]",
        title="tmux Not Installed",
        message=(
            "PAIR terminal backend is set to tmux, but tmux was not found in PATH.\n"
            "PAIR sessions will not open until tmux is installed."
        ),
        hint="Install tmux or switch PAIR terminal backend to WezTerm",
    ),
    IssueType.WEZTERM_MISSING: IssuePreset(
        type=IssueType.WEZTERM_MISSING,
        severity=IssueSeverity.WARNING,
        icon="[~]",
        title="WezTerm Not Installed",
        message=(
            "PAIR terminal backend is set to WezTerm, but wezterm was not found in PATH.\n"
            "PAIR sessions will not open until WezTerm is installed."
        ),
        hint="Install WezTerm or switch PAIR terminal backend to tmux",
        url="https://wezterm.org/install/",
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
    preset: IssuePreset
    details: str | None = None


@dataclass
class PreflightResult:
    issues: list[DetectedIssue]

    @property
    def has_blocking_issues(self) -> bool:
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

    resolved_command: list[str] | None
    issue: DetectedIssue | None
    used_fallback: bool = False


def _is_npx_command(command: str) -> bool:
    try:
        parts = split_command_string(command)
        return len(parts) > 0 and parts[0] == "npx"
    except ValueError:
        return command.startswith("npx ")


def _get_npx_package_binary(command: str) -> str | None:
    """Extract the binary name from an npx command.

    For "npx claude-code-acp", returns "claude-code-acp".
    For "npx @anthropic-ai/claude-code-acp", returns "claude-code-acp".
    """
    try:
        parts = split_command_string(command)
        if len(parts) < 2:
            return None
        package = parts[1]

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

        binary_path = shutil.which(binary_name)
        if binary_path is not None:
            try:
                parts = split_command_string(run_command)
                resolved = [binary_path, *parts[2:]]
            except ValueError:
                resolved = [binary_path]
            return ACPCommandResolution(
                resolved_command=resolved,
                issue=None,
                used_fallback=True,
            )

        npx_resolved = shutil.which("npx")
        if npx_resolved is not None:
            from kagan.command_utils import ensure_windows_npm_dir

            ensure_windows_npm_dir()
            try:
                parts = split_command_string(run_command)
                resolved = [npx_resolved, *parts[1:]]
            except ValueError:
                resolved = [npx_resolved]
            return ACPCommandResolution(
                resolved_command=resolved,
                issue=None,
                used_fallback=False,
            )

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

    try:
        parts = split_command_string(run_command)
        executable = parts[0] if parts else run_command
    except ValueError:
        executable = run_command

    if shutil.which(executable) is not None:
        return ACPCommandResolution(
            resolved_command=split_command_string(run_command),
            issue=None,
            used_fallback=False,
        )

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


def _command_exists(command: str) -> bool:
    candidates = [command]

    if platform.system() == "Windows":
        command_lower = command.lower()
        if not command_lower.endswith((".exe", ".cmd", ".bat")):
            candidates.extend([f"{command}.exe", f"{command}.cmd", f"{command}.bat"])

    return any(shutil.which(candidate) is not None for candidate in candidates)


def _resolve_pair_terminal_backend(
    pair_terminal_backend: str | None,
    default_pair_terminal_backend: str | None,
) -> str:
    backend = (pair_terminal_backend or default_pair_terminal_backend or "tmux").strip().lower()
    if backend in {"tmux", "vscode", "cursor"}:
        if backend == "tmux" and platform.system() == "Windows":
            for candidate, command in (
                ("vscode", "code"),
                ("cursor", "cursor"),
            ):
                if _command_exists(command):
                    return candidate
            return "vscode"
        return backend
    return "tmux"


def _tmux_install_hint() -> tuple[str, str | None]:
    system = platform.system()
    if system == "Darwin":
        return "Install tmux: brew install tmux", "https://github.com/tmux/tmux/wiki/Installing"
    if system == "Windows":
        return (
            "tmux is typically used via WSL2 on Windows. "
            "Install WSL2, then install tmux inside WSL, "
            "or switch PAIR terminal backend to VS Code/Cursor.",
            "https://learn.microsoft.com/windows/wsl/install",
        )
    return (
        "Install tmux (for example: sudo apt install tmux), "
        "or switch PAIR terminal backend to VS Code/Cursor.",
        "https://github.com/tmux/tmux/wiki/Installing",
    )


def _wezterm_install_hint() -> tuple[str, str | None]:
    system = platform.system()
    if system == "Darwin":
        return (
            "Install WezTerm: brew install --cask wezterm",
            "https://wezterm.org/install/macos.html",
        )
    if system == "Windows":
        return (
            "Install WezTerm: winget install --id Wez.WezTerm -e",
            "https://wezterm.org/install/windows.html",
        )
    return (
        "Install WezTerm from your package manager or https://wezterm.org/install/ "
        "(for example: sudo apt install wezterm).",
        "https://wezterm.org/install/",
    )


def _check_tmux() -> DetectedIssue | None:
    if not _command_exists("tmux"):
        hint, url = _tmux_install_hint()
        preset = IssuePreset(
            type=IssueType.TMUX_MISSING,
            severity=IssueSeverity.WARNING,
            icon="[~]",
            title="tmux Not Installed",
            message=(
                "PAIR terminal backend is set to tmux, but tmux was not found in PATH.\n"
                "PAIR sessions will not open until tmux is installed."
            ),
            hint=hint,
            url=url,
        )
        return DetectedIssue(preset=preset, details="tmux")
    return None


def _check_wezterm() -> DetectedIssue | None:
    if not _command_exists("wezterm"):
        hint, url = _wezterm_install_hint()
        preset = IssuePreset(
            type=IssueType.WEZTERM_MISSING,
            severity=IssueSeverity.WARNING,
            icon="[~]",
            title="WezTerm Not Installed",
            message=(
                "PAIR terminal backend is set to WezTerm, but wezterm was not found in PATH.\n"
                "PAIR sessions will not open until WezTerm is installed."
            ),
            hint=hint,
            url=url,
        )
        return DetectedIssue(preset=preset, details="wezterm")
    return None


def _check_pair_terminal_backend(
    *,
    pair_terminal_backend: str | None,
    default_pair_terminal_backend: str | None,
) -> DetectedIssue | None:
    backend = _resolve_pair_terminal_backend(pair_terminal_backend, default_pair_terminal_backend)
    if backend == "tmux":
        return _check_tmux()
    return None


def _check_agent(
    agent_command: str,
    agent_name: str,
    install_command: str | None,
) -> DetectedIssue | None:
    try:
        parts = split_command_string(agent_command)
        executable = parts[0] if parts else agent_command
    except ValueError:
        executable = agent_command

    if shutil.which(executable) is None:
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
    from kagan.git_utils import MIN_GIT_VERSION, get_git_version

    version = await get_git_version()
    if version is None:
        return DetectedIssue(preset=ISSUE_PRESETS[IssueType.GIT_NOT_INSTALLED])

    if version < MIN_GIT_VERSION:
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
    from kagan.git_utils import check_git_user_configured

    is_configured, error_msg = await check_git_user_configured()
    if not is_configured:
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
    agent_config: AgentConfig | None = None,
    agent_name: str = "Claude Code",
    agent_install_command: str | None = None,
    pair_terminal_backend: str | None = None,
    default_pair_terminal_backend: str | None = "tmux",
    check_git: bool = True,
    check_terminal: bool = True,
) -> PreflightResult:
    """Run all pre-flight checks and return detected issues.

    Args:
        agent_config: Optional agent configuration to check.
        agent_name: Display name of the agent to check.
        agent_install_command: Installation command for the agent.
        pair_terminal_backend: Explicit PAIR terminal backend to validate for the current context.
        default_pair_terminal_backend: Configured default PAIR backend when no task override exists.
        check_git: Whether to check git version and configuration.
        check_terminal: Whether to check terminal truecolor support.

    Returns:
        PreflightResult containing all detected issues.
    """
    issues: list[DetectedIssue] = []

    if check_git:
        git_version_issue = await _check_git_version()
        if git_version_issue:
            issues.append(git_version_issue)
        else:
            git_user_issue = await _check_git_user()
            if git_user_issue:
                issues.append(git_user_issue)

    pair_terminal_issue = _check_pair_terminal_backend(
        pair_terminal_backend=pair_terminal_backend,
        default_pair_terminal_backend=default_pair_terminal_backend,
    )
    if pair_terminal_issue:
        issues.append(pair_terminal_issue)

    # 4. Terminal truecolor check (warning only)
    if check_terminal:
        terminal_issue = _check_terminal_truecolor()
        if terminal_issue:
            issues.append(terminal_issue)

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
    from kagan.builtin_agents import list_builtin_agents

    issues = []
    for agent in list_builtin_agents():
        preset = IssuePreset(
            type=IssueType.NO_AGENTS_AVAILABLE,
            severity=IssueSeverity.BLOCKING,
            icon="[+]",
            title=f"Install {agent.config.name}",
            message=f"{agent.description}\nBy {agent.author}",
            hint=agent.install_command,
            url=agent.docs_url if agent.docs_url else None,
        )
        issues.append(DetectedIssue(preset=preset))

    return issues
