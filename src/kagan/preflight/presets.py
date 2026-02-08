"""Preflight issue models and preset definitions."""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum


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


def create_no_agents_issues() -> list[DetectedIssue]:
    """Create issues showing all available agent install options."""
    from kagan.builtin_agents import list_builtin_agents

    issues: list[DetectedIssue] = []
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
