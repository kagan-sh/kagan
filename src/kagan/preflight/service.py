"""Preflight orchestration service."""

from __future__ import annotations

from typing import TYPE_CHECKING

from .checks import (
    _check_agent,
    _check_git_user,
    _check_git_version,
    _check_pair_terminal_backend,
    _check_terminal_truecolor,
)
from .presets import DetectedIssue, PreflightResult
from .resolution import resolve_acp_command

if TYPE_CHECKING:
    from kagan.config import AgentConfig


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
    """Run all pre-flight checks and return detected issues."""
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
