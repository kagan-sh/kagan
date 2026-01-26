"""Built-in agent definitions for Kagan."""

from __future__ import annotations

from dataclasses import dataclass

from kagan.config import AgentConfig


@dataclass
class BuiltinAgent:
    """Extended agent info with metadata for welcome screen."""

    config: AgentConfig
    author: str
    description: str
    install_command: str


# Built-in agents that ship with Kagan
# Run commands use ACP (Agent Communication Protocol) for standardized communication
BUILTIN_AGENTS: dict[str, BuiltinAgent] = {
    "claude": BuiltinAgent(
        config=AgentConfig(
            identity="anthropic.claude",
            name="Claude Code",
            short_name="claude",
            run_command={"*": "claude-code-acp"},
            active=True,
        ),
        author="Anthropic",
        description="Agentic AI for coding tasks",
        install_command=(
            "npm install -g @anthropic/claude-code && npm install -g @anthropic/claude-code-acp"
        ),
    ),
    "opencode": BuiltinAgent(
        config=AgentConfig(
            identity="opencode.dev",
            name="OpenCode",
            short_name="opencode",
            run_command={"*": "opencode acp"},
            active=True,
        ),
        author="Open Source",
        description="Multi-model CLI with TUI",
        install_command="npm i -g opencode-ai",
    ),
    "codex": BuiltinAgent(
        config=AgentConfig(
            identity="openai.codex",
            name="Codex CLI",
            short_name="codex",
            run_command={"*": "npx @zed-industries/codex-acp"},
            active=True,
        ),
        author="OpenAI",
        description="AI coding assistant",
        install_command="npm install -g @openai/codex",
    ),
    "gemini": BuiltinAgent(
        config=AgentConfig(
            identity="google.gemini",
            name="Gemini CLI",
            short_name="gemini",
            run_command={"*": "gemini --experimental-acp"},
            active=True,
        ),
        author="Google",
        description="Multimodal AI agent",
        install_command="npm install -g @google/gemini-cli",
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
