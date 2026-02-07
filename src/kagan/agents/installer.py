"""Agent installer functionality.

Provides utilities to check if coding agents are installed
and install them via their official installation methods.

Supports: Claude Code, OpenCode, Codex, Gemini CLI, Kimi CLI, GitHub Copilot.
"""

from __future__ import annotations

import asyncio
import shutil
from enum import StrEnum
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Awaitable, Callable

    from kagan.builtin_agents import BuiltinAgent


INSTALL_TIMEOUT_SECONDS = 120


class AgentType(StrEnum):
    """Supported agent types."""

    CLAUDE = "claude"
    OPENCODE = "opencode"
    CODEX = "codex"
    GEMINI = "gemini"
    KIMI = "kimi"
    COPILOT = "copilot"


# Agents that require npm for installation
_NPM_AGENTS: frozenset[AgentType] = frozenset(
    {AgentType.OPENCODE, AgentType.CODEX, AgentType.GEMINI, AgentType.COPILOT}
)

# Agents that require uv for installation
_UV_AGENTS: frozenset[AgentType] = frozenset({AgentType.KIMI})


class InstallerError(Exception):
    """Raised when installation operations fail."""


def _get_builtin_agent(agent: AgentType | str) -> BuiltinAgent:
    """Get builtin agent or raise ValueError if not found."""
    from kagan.builtin_agents import get_builtin_agent

    builtin = get_builtin_agent(str(agent))
    if not builtin:
        valid = list(AgentType)
        raise ValueError(f"Unsupported agent: {agent}. Supported agents: {valid}")
    return builtin


def check_agent_installed(agent: AgentType | str) -> bool:
    """Check if an agent's CLI executable is available in PATH.

    Args:
        agent: The agent to check. One of: claude, opencode, codex, gemini, kimi, copilot.

    Returns:
        True if the agent's CLI is available, False otherwise.

    Raises:
        ValueError: If an unsupported agent is specified.
    """
    builtin = _get_builtin_agent(agent)
    # Extract executable from interactive_command (e.g., "claude" from "claude")
    from kagan.config import get_os_value

    cmd = get_os_value(builtin.config.interactive_command)
    if not cmd:
        return False
    executable = cmd.split()[0]
    return shutil.which(executable) is not None


def _check_prerequisites(agent: AgentType) -> str | None:
    """Check if prerequisites for installing an agent are met.

    Args:
        agent: The agent to check prerequisites for.

    Returns:
        Error message if prerequisites are missing, None if all prerequisites are met.
    """
    if agent in _NPM_AGENTS:
        if shutil.which("npm") is None:
            return "npm is not installed. Please install Node.js and npm first: https://nodejs.org/"
    elif agent in _UV_AGENTS:
        if shutil.which("uv") is None:
            return (
                "uv is not installed. "
                "Please install uv first: https://docs.astral.sh/uv/getting-started/installation/"
            )
    # claude uses curl which is typically available on all systems
    return None


def _get_path_hint(agent: AgentType) -> str:
    """Get PATH hint message for post-installation.

    Args:
        agent: The agent that was installed.

    Returns:
        Hint string about PATH configuration.
    """
    if agent in _NPM_AGENTS:
        return "You may need to add npm global bin directory to your PATH."
    elif agent in _UV_AGENTS:
        return "You may need to add ~/.local/bin to your PATH."
    else:
        # claude
        return "You may need to restart your shell or add claude to your PATH."


def get_install_command(agent: AgentType | str = AgentType.CLAUDE) -> str:
    """Return the install command string for display.

    Args:
        agent: The agent to get the install command for.
               One of: claude, opencode, codex, gemini, kimi, copilot.
               Defaults to "claude".

    Returns:
        The install command for the specified agent.

    Raises:
        ValueError: If an unsupported agent is specified.
    """
    builtin = _get_builtin_agent(agent)
    return builtin.install_command


async def check_claude_code_installed() -> bool:
    """Check if the `claude` command exists in PATH.

    Returns:
        True if claude command is available, False otherwise.
    """

    return shutil.which("claude") is not None


def check_opencode_installed() -> bool:
    """Check if OpenCode CLI is available in PATH.

    Returns:
        True if opencode command is available, False otherwise.
    """
    return shutil.which("opencode") is not None


async def _run_install(
    command: str,
    verify_fn: Callable[[], bool | Awaitable[bool]],
    agent_name: str,
    success_msg: str,
    path_hint: str,
    timeout: float,
) -> tuple[bool, str]:
    """Run an installation command and verify success.

    Args:
        command: Shell command to execute
        verify_fn: Function to verify installation (sync or async)
        agent_name: Human-readable agent name for error messages
        success_msg: Message to return on success
        path_hint: Hint about PATH if command not found after install
        timeout: Maximum time in seconds to wait

    Returns:
        A tuple of (success, message)
    """
    try:
        proc = await asyncio.create_subprocess_shell(
            command,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )

        try:
            stdout, stderr = await asyncio.wait_for(
                proc.communicate(),
                timeout=timeout,
            )
        except TimeoutError:
            proc.kill()
            await proc.wait()
            return False, f"Installation timed out after {timeout} seconds"

        stdout_str = stdout.decode().strip()
        stderr_str = stderr.decode().strip()

        if proc.returncode == 0:
            result = verify_fn()
            if asyncio.iscoroutine(result):
                verified = await result
            else:
                verified = result

            if verified:
                return True, success_msg
            else:
                return True, f"Installation completed. {path_hint}"
        else:
            error_details = []
            if stderr_str:
                error_details.append(stderr_str)
            if stdout_str:
                error_details.append(stdout_str)

            error_message = (
                "; ".join(error_details)
                if error_details
                else f"Installation failed with exit code {proc.returncode}"
            )
            return False, f"Installation failed: {error_message}"

    except FileNotFoundError:
        return False, f"Installation failed: required command not found for {agent_name}"
    except OSError as e:
        return False, f"Installation failed: {e}"


async def install_claude_code(
    timeout: float = INSTALL_TIMEOUT_SECONDS,
) -> tuple[bool, str]:
    """Install Claude Code using the official install script.

    Runs `curl -fsSL https://claude.ai/install.sh | sh` in the user's shell.

    Args:
        timeout: Maximum time in seconds to wait for installation.
                 Defaults to 120 seconds.

    Returns:
        A tuple of (success, message) where:
        - success: True if installation completed successfully
        - message: Descriptive message about the result or error
    """
    builtin = _get_builtin_agent(AgentType.CLAUDE)
    return await _run_install(
        command=builtin.install_command,
        verify_fn=check_claude_code_installed,
        agent_name=builtin.config.name,
        success_msg="Claude Code installed successfully",
        path_hint="You may need to restart your shell or add claude to your PATH.",
        timeout=timeout,
    )


async def install_opencode(
    timeout: float = INSTALL_TIMEOUT_SECONDS,
) -> tuple[bool, str]:
    """Install OpenCode using npm.

    Runs `npm i -g opencode-ai` to install OpenCode globally.

    Args:
        timeout: Maximum time in seconds to wait for installation.
                 Defaults to 120 seconds.

    Returns:
        A tuple of (success, message) where:
        - success: True if installation completed successfully
        - message: Descriptive message about the result or error
    """
    if shutil.which("npm") is None:
        return False, (
            "Installation failed: npm is not installed. "
            "Please install Node.js and npm first: https://nodejs.org/"
        )

    builtin = _get_builtin_agent(AgentType.OPENCODE)
    return await _run_install(
        command=builtin.install_command,
        verify_fn=check_opencode_installed,
        agent_name=builtin.config.name,
        success_msg="OpenCode installed successfully",
        path_hint="You may need to add npm global bin directory to your PATH.",
        timeout=timeout,
    )


async def install_agent(
    agent: AgentType | str,
    timeout: float = INSTALL_TIMEOUT_SECONDS,
) -> tuple[bool, str]:
    """Install the specified coding agent.

    Generic installer that works for all supported agents.
    Checks prerequisites, runs install command, and verifies installation.

    Args:
        agent: The agent to install. One of: claude, opencode, codex, gemini, kimi, copilot.
        timeout: Maximum time in seconds to wait for installation.
                 Defaults to 120 seconds.

    Returns:
        A tuple of (success, message) where:
        - success: True if installation completed successfully
        - message: Descriptive message about the result or error

    Raises:
        ValueError: If an unsupported agent is specified.
    """
    builtin = _get_builtin_agent(agent)
    agent_type = AgentType(str(agent))

    # Check prerequisites
    prereq_error = _check_prerequisites(agent_type)
    if prereq_error:
        return False, f"Installation failed: {prereq_error}"

    agent_name = builtin.config.name
    path_hint = _get_path_hint(agent_type)

    return await _run_install(
        command=builtin.install_command,
        verify_fn=lambda: check_agent_installed(agent_type),
        agent_name=agent_name,
        success_msg=f"{agent_name} installed successfully",
        path_hint=path_hint,
        timeout=timeout,
    )
