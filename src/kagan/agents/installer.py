"""Agent installer functionality.

Provides utilities to check if coding agents (Claude Code, OpenCode) are installed
and install them via their official installation methods.
"""

from __future__ import annotations

import asyncio
import shutil
from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from collections.abc import Awaitable, Callable

# Default timeout for installation (2 minutes)
INSTALL_TIMEOUT_SECONDS = 120

# Supported agent types
type AgentType = Literal["claude", "opencode"]

# Agent-specific install commands
INSTALL_COMMANDS: dict[AgentType, str] = {
    "claude": "curl -fsSL https://claude.ai/install.sh | sh",
    "opencode": "npm i -g opencode-ai",
}

# Legacy constant for backward compatibility
INSTALL_COMMAND = INSTALL_COMMANDS["claude"]


class InstallerError(Exception):
    """Raised when installation operations fail."""


def get_install_command(agent: AgentType = "claude") -> str:
    """Return the install command string for display.

    Args:
        agent: The agent to get the install command for.
               Supported values: "claude", "opencode".
               Defaults to "claude".

    Returns:
        The install command for the specified agent.

    Raises:
        ValueError: If an unsupported agent is specified.
    """
    if agent not in INSTALL_COMMANDS:
        raise ValueError(
            f"Unsupported agent: {agent}. Supported agents: {list(INSTALL_COMMANDS.keys())}"
        )
    return INSTALL_COMMANDS[agent]


async def check_claude_code_installed() -> bool:
    """Check if the `claude` command exists in PATH.

    Returns:
        True if claude command is available, False otherwise.
    """
    # Use shutil.which for synchronous check - it's fast enough
    # and doesn't require spawning a subprocess
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
            # Verify installation - handle both sync and async verify functions
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
    return await _run_install(
        command=INSTALL_COMMANDS["claude"],
        verify_fn=check_claude_code_installed,
        agent_name="Claude Code",
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
    # Check if npm is available before attempting installation
    if shutil.which("npm") is None:
        return False, (
            "Installation failed: npm is not installed. "
            "Please install Node.js and npm first: https://nodejs.org/"
        )

    return await _run_install(
        command=INSTALL_COMMANDS["opencode"],
        verify_fn=check_opencode_installed,
        agent_name="OpenCode",
        success_msg="OpenCode installed successfully",
        path_hint="You may need to add npm global bin directory to your PATH.",
        timeout=timeout,
    )


async def install_agent(
    agent: AgentType,
    timeout: float = INSTALL_TIMEOUT_SECONDS,
) -> tuple[bool, str]:
    """Install the specified coding agent.

    This is a generic dispatcher that routes to the appropriate
    agent-specific installer.

    Args:
        agent: The agent to install. Supported values: "claude", "opencode".
        timeout: Maximum time in seconds to wait for installation.
                 Defaults to 120 seconds.

    Returns:
        A tuple of (success, message) where:
        - success: True if installation completed successfully
        - message: Descriptive message about the result or error

    Raises:
        ValueError: If an unsupported agent is specified.
    """
    if agent == "claude":
        return await install_claude_code(timeout=timeout)
    elif agent == "opencode":
        return await install_opencode(timeout=timeout)
    else:
        raise ValueError(
            f"Unsupported agent: {agent}. Supported agents: {list(INSTALL_COMMANDS.keys())}"
        )
