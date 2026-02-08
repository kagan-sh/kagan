"""Focused preflight checks."""

from __future__ import annotations

import platform

from kagan.command_utils import cached_which, split_command_string

from .presets import ISSUE_PRESETS, DetectedIssue, IssuePreset, IssueSeverity, IssueType


def _command_exists(command: str) -> bool:
    candidates = [command]

    if platform.system() == "Windows":
        command_lower = command.lower()
        if not command_lower.endswith((".exe", ".cmd", ".bat")):
            candidates.extend([f"{command}.exe", f"{command}.cmd", f"{command}.bat"])

    return any(cached_which(candidate) is not None for candidate in candidates)


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

    if cached_which(executable) is None:
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
