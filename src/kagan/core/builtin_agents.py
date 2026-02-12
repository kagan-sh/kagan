"""Built-in agent definitions for Kagan."""

from __future__ import annotations

import shlex
import shutil
from dataclasses import dataclass, field

from kagan.core.agents.backend_config import (
    AgentBackendConfig,
    ClaudeAgentConfig,
    CodexAgentConfig,
    CopilotAgentConfig,
    GeminiAgentConfig,
    KimiAgentConfig,
    OpenCodeAgentConfig,
)
from kagan.core.config import AgentConfig, get_os_value


@dataclass
class BuiltinAgent:
    """Extended agent info with metadata for welcome screen."""

    config: AgentConfig
    author: str
    description: str
    install_command: str
    docs_url: str = ""
    mcp_config_file: str = ".mcp.json"
    mcp_config_format: str = "claude"
    backend_config: AgentBackendConfig = field(default_factory=ClaudeAgentConfig)


@dataclass
class AgentAvailability:
    """Availability status for an agent."""

    agent: BuiltinAgent
    interactive_available: bool = False
    acp_available: bool = False

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


AGENT_PRIORITY = ["claude", "opencode", "codex", "gemini", "kimi", "copilot"]

BUILTIN_AGENTS: dict[str, BuiltinAgent] = {
    "claude": BuiltinAgent(
        config=AgentConfig(
            identity="claude.com",
            name="Claude Code",
            short_name="claude",
            run_command={"*": "npx claude-code-acp"},
            interactive_command={"*": "claude"},
            active=True,
            model_env_var="ANTHROPIC_MODEL",
        ),
        author="Anthropic",
        description="Agentic AI for coding tasks",
        install_command="curl -fsSL https://claude.ai/install.sh | bash",
        docs_url="https://docs.anthropic.com/en/docs/claude-code",
        mcp_config_file=".mcp.json",
        mcp_config_format="claude",
        backend_config=ClaudeAgentConfig(),
    ),
    "opencode": BuiltinAgent(
        config=AgentConfig(
            identity="opencode.ai",
            name="OpenCode",
            short_name="opencode",
            run_command={"*": "opencode acp"},
            interactive_command={"*": "opencode"},
            active=True,
            model_env_var="",
        ),
        author="SST",
        description="Multi-model CLI with TUI",
        install_command="npm i -g opencode-ai",
        docs_url="https://opencode.ai/docs",
        mcp_config_file="opencode.json",
        mcp_config_format="opencode",
        backend_config=OpenCodeAgentConfig(),
    ),
    "codex": BuiltinAgent(
        config=AgentConfig(
            identity="codex.openai.com",
            name="Codex",
            short_name="codex",
            run_command={"*": "npx @zed-industries/codex-acp"},
            interactive_command={"*": "codex"},
            active=True,
            model_env_var="",
        ),
        author="OpenAI",
        description="OpenAI CLI coding agent",
        install_command="npm install -g @openai/codex",
        docs_url="https://github.com/openai/codex",
        backend_config=CodexAgentConfig(),
    ),
    "gemini": BuiltinAgent(
        config=AgentConfig(
            identity="gemini.google.com",
            name="Gemini CLI",
            short_name="gemini",
            run_command={"*": "gemini --experimental-acp"},
            interactive_command={"*": "gemini"},
            active=True,
            model_env_var="",
        ),
        author="Google",
        description="Google Gemini CLI agent",
        install_command="npm install -g @google/gemini-cli",
        docs_url="https://github.com/google-gemini/gemini-cli",
        backend_config=GeminiAgentConfig(),
    ),
    "kimi": BuiltinAgent(
        config=AgentConfig(
            identity="kimi.moonshot.cn",
            name="Kimi CLI",
            short_name="kimi",
            run_command={"*": "kimi acp"},
            interactive_command={"*": "kimi"},
            active=True,
            model_env_var="",
        ),
        author="Moonshot AI",
        description="Kimi CLI coding agent",
        install_command="uv tool install kimi-cli --no-cache",
        docs_url="https://github.com/MoonshotAI/kimi-cli",
        backend_config=KimiAgentConfig(),
    ),
    "copilot": BuiltinAgent(
        config=AgentConfig(
            identity="copilot.github.com",
            name="GitHub Copilot",
            short_name="copilot",
            run_command={"*": "copilot --acp"},
            interactive_command={"*": "copilot"},
            active=True,
            model_env_var="",
        ),
        author="GitHub",
        description="GitHub Copilot CLI agent",
        install_command="npm install -g @github/copilot@prerelease",
        docs_url="https://github.com/github/copilot-cli",
        mcp_config_file=".mcp.json",
        mcp_config_format="claude",
        backend_config=CopilotAgentConfig(),
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
    """Check if a command's executable is available in PATH."""
    if not command:
        return False

    try:
        parts = shlex.split(command)
        executable = parts[0] if parts else command
    except ValueError:
        return shutil.which(command) is not None

    if executable == "npx" and len(parts) > 1:
        package = parts[1]

        binary = package.split("/")[-1] if "/" in package else package

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


def list_available_agents() -> list[BuiltinAgent]:
    """Return list of agents that are currently installed.

    Checks if each agent's interactive command executable is available in PATH.

    Returns:
        List of BuiltinAgent objects for agents that are installed.
    """
    available = []
    for availability in get_all_agent_availability():
        if availability.is_available:
            available.append(availability.agent)
    return available


def get_agent_status() -> dict[str, bool]:
    """Return availability status for all agents.

    Returns:
        Dictionary mapping agent short_name to availability boolean.
        Example: {"claude": True, "opencode": False, "codex": False, ...}
    """
    return {
        agent.config.short_name: availability.is_available
        for agent in BUILTIN_AGENTS.values()
        if (availability := check_agent_availability(agent))
    }
