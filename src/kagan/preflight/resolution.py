"""ACP command resolution for preflight checks."""

from __future__ import annotations

from dataclasses import dataclass

from kagan.command_utils import cached_which, split_command_string

from .presets import DetectedIssue, IssuePreset, IssueSeverity, IssueType


@dataclass
class ACPCommandResolution:
    """Result of resolving an ACP command."""

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
    """Extract the binary name from an npx command."""
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
    """Resolve an ACP command, handling npx fallback scenarios."""
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

        binary_path = cached_which(binary_name)
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

        npx_resolved = cached_which("npx")
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

    if cached_which(executable) is not None:
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
