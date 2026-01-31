"""Built-in agent definitions for Kagan."""

from __future__ import annotations

import shlex
import shutil
from dataclasses import dataclass

from kagan.config import AgentConfig, get_os_value


@dataclass
class BuiltinAgent:
    """Extended agent info with metadata for welcome screen."""

    config: AgentConfig
    author: str
    description: str
    install_command: str
    docs_url: str = ""  # Documentation URL for troubleshooting
    mcp_config_file: str = ".mcp.json"  # Filename for MCP config
    mcp_config_format: str = "claude"  # Format: "claude" | "opencode"


@dataclass
class AgentAvailability:
    """Availability status for an agent."""

    agent: BuiltinAgent
    interactive_available: bool = False  # CLI available for PAIR mode
    acp_available: bool = False  # ACP command available for AUTO mode

    @property
    def is_available(self) -> bool:
        """Check if agent is available in any mode."""
        return self.interactive_available or self.acp_available

    @property
    def install_hint(self) -> str:
        """Get one-liner install instruction."""
        return self.agent.install_command

    @property
    def docs_url(self) -> str:
        """Get documentation URL."""
        return self.agent.docs_url


# Built-in agents that ship with Kagan
# run_command: ACP protocol command for AUTO mode (programmatic)
# interactive_command: CLI command for PAIR mode (interactive tmux session)
# NOTE: Only OpenCode and Claude Code are supported. Other CLI tools removed.
#
# MCP config formats:
# - claude: Uses .mcp.json with {"mcpServers": {"name": {"command": ..., "args": [...]}}}
# - opencode: Uses opencode.json with {"mcp": {"name": {"type": "local", "command": [...]}}}

# Agent priority order for auto-selection (first available wins)
AGENT_PRIORITY = ["claude", "opencode"]

BUILTIN_AGENTS: dict[str, BuiltinAgent] = {
    "claude": BuiltinAgent(
        config=AgentConfig(
            identity="claude.com",
            name="Claude Code",
            short_name="claude",
            run_command={"*": "npx claude-code-acp"},
            interactive_command={"*": "claude"},
            active=True,
        ),
        author="Anthropic",
        description="Agentic AI for coding tasks",
        install_command="curl -fsSL https://claude.ai/install.sh | bash",
        docs_url="https://docs.anthropic.com/en/docs/claude-code",
        mcp_config_file=".mcp.json",
        mcp_config_format="claude",
    ),
    "opencode": BuiltinAgent(
        config=AgentConfig(
            identity="opencode.ai",
            name="OpenCode",
            short_name="opencode",
            run_command={"*": "opencode acp"},
            interactive_command={"*": "opencode"},
            active=True,
        ),
        author="SST",
        description="Multi-model CLI with TUI",
        install_command="npm i -g opencode-ai",
        docs_url="https://opencode.ai/docs",
        mcp_config_file="opencode.json",
        mcp_config_format="opencode",
    ),
}


def get_builtin_agent(name: str) -> BuiltinAgent | None:
    """Get a built-in agent by short name.

    Args:
        name: The short name of the agent (e.g., 'claude', 'opencode').

    Returns:
        The BuiltinAgent if found, None otherwise.
    """
    return BUILTIN_AGENTS.get(name)


def list_builtin_agents() -> list[BuiltinAgent]:
    """Get all built-in agents.

    Returns:
        A list of all BuiltinAgent objects.
    """
    return list(BUILTIN_AGENTS.values())


def _check_command_available(command: str | None) -> bool:
    """Check if a command's executable is available in PATH.

    Handles both simple commands ('claude') and complex commands
    ('npx claude-code-acp', 'opencode acp').

    For npx commands, checks if either npx or the package binary is available.
    """
    if not command:
        return False

    try:
        parts = shlex.split(command)
        executable = parts[0] if parts else command
    except ValueError:
        # If parsing fails, treat the whole command as executable
        return shutil.which(command) is not None

    # Handle npx commands specially
    # For availability checking, we require the binary to be globally installed
    # Just having npx is not sufficient - the package must be installed
    if executable == "npx" and len(parts) > 1:
        package = parts[1]
        # Check if package is globally installed (for scoped packages like @org/pkg)
        binary = package.split("/")[-1] if "/" in package else package
        # Only consider available if the binary is globally installed
        # npx can run packages on demand, but we can't verify that without running it
        return shutil.which(binary) is not None

    return shutil.which(executable) is not None


def check_agent_availability(agent: BuiltinAgent) -> AgentAvailability:
    """Check if an agent's commands are available in PATH.

    Args:
        agent: The BuiltinAgent to check.

    Returns:
        AgentAvailability with status for both interactive and ACP modes.
    """
    interactive_cmd = get_os_value(agent.config.interactive_command)
    acp_cmd = get_os_value(agent.config.run_command)

    return AgentAvailability(
        agent=agent,
        interactive_available=_check_command_available(interactive_cmd),
        acp_available=_check_command_available(acp_cmd),
    )


def get_all_agent_availability() -> list[AgentAvailability]:
    """Get availability status for all built-in agents.

    Returns agents in priority order (claude first, then opencode).

    Returns:
        List of AgentAvailability for all agents in priority order.
    """
    result = []
    for key in AGENT_PRIORITY:
        if agent := BUILTIN_AGENTS.get(key):
            result.append(check_agent_availability(agent))
    return result


def get_first_available_agent() -> BuiltinAgent | None:
    """Get the first available agent based on priority.

    Priority: claude > opencode

    Returns:
        The first available BuiltinAgent, or None if none available.
    """
    for availability in get_all_agent_availability():
        if availability.is_available:
            return availability.agent
    return None


def any_agent_available() -> bool:
    """Check if any agent is available.

    Returns:
        True if at least one agent is available.
    """
    return any(a.is_available for a in get_all_agent_availability())
